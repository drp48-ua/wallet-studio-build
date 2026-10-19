"""Private immutable backup service. Python standard library; HTTPS proxy in production."""
import argparse,contextlib,hashlib,hmac,json,os,pathlib,re,secrets,sqlite3,threading,time,uuid,zipfile,io,getpass,sys
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from accounts import Accounts,AccountError
from designs import DesignCatalog,catalog_for_client,design_for_client,EXPANDED_CATALOG_HEADER
from remote import PgDatabase,SupabaseBlobs,RemoteError,MAX_REMOTE_BUNDLE,StartupError,startup_stage

HEX=re.compile(r'^[0-9a-f]{64}$')
MAX_BUNDLE=100*1024*1024
PASSWORD_ROUNDS=600_000
class Store:
    def __init__(self,root,allow_registration=False,session_seconds=86400*7):
        self.design_catalog=DesignCatalog(dict(os.environ))
        self.remote=os.environ.get('WALLET_STORAGE_MODE')=='remote'
        self.remote_db=None;self.blob_store=None;self.max_bundle=MAX_REMOTE_BUNDLE if self.remote else MAX_BUNDLE
        if os.environ.get('RENDER') and not self.remote:raise StartupError('configuration',ValueError())
        if self.remote:
            with startup_stage('configuration'):
                required=('DATABASE_URL','SUPABASE_URL','SUPABASE_SERVICE_ROLE_KEY','SUPABASE_STORAGE_BUCKET')
                if not all(os.environ.get(key) for key in required):raise ValueError('remote_storage_configuration_required')
                if allow_registration:raise ValueError('remote_registration_fixture_forbidden')
                self.remote_db=PgDatabase(os.environ['DATABASE_URL']);self.lock=self.remote_db.lock
            with startup_stage('storage_configuration'):
                self.blob_store=SupabaseBlobs(os.environ['SUPABASE_URL'],os.environ['SUPABASE_SERVICE_ROLE_KEY'],os.environ['SUPABASE_STORAGE_BUCKET'])
            with startup_stage('database_schema'):self.remote_db.initialize()
            with startup_stage('storage_bucket'):self.blob_store.verify_bucket()
            self.allow_registration=False;self.session_seconds=session_seconds;self.limits={}
            with startup_stage('account_schema'):self.accounts=Accounts(self)
            with startup_stage('database_permissions'):self.remote_db.secure_tables()
            return
        self.root=pathlib.Path(root).absolute()
        if os.name=='nt' and not str(self.root).startswith('\\\\?\\'):
            self.root=pathlib.Path('\\\\?\\'+str(self.root))
        self.root.mkdir(parents=True,exist_ok=True)
        self.blobs=self.root/'blobs';self.blobs.mkdir(exist_ok=True)
        if os.name!='nt':
            with contextlib.suppress(OSError):os.chmod(self.root,0o700)
        self.db=self.root/'accounts.sqlite3';self.allow_registration=allow_registration;self.session_seconds=session_seconds
        self.lock=threading.RLock();self.limits={}
        with self.connect() as db:
            db.executescript('''PRAGMA journal_mode=WAL;
              CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,email TEXT UNIQUE NOT NULL,salt BLOB NOT NULL,password BLOB NOT NULL);
              CREATE TABLE IF NOT EXISTS sessions(digest TEXT PRIMARY KEY,user_id TEXT NOT NULL,expires INTEGER NOT NULL);
              CREATE TABLE IF NOT EXISTS bundles(user_id TEXT NOT NULL,id TEXT NOT NULL,size INTEGER NOT NULL,PRIMARY KEY(user_id,id));''')
        self.accounts=Accounts(self)
    @contextlib.contextmanager
    def connect(self):
        if self.remote_db:
            with self.remote_db.connect() as db:yield db
            return
        db=sqlite3.connect(self.db,timeout=10)
        try:
            with db:yield db
        finally:db.close()
    def rate(self,peer,email):
        now=time.monotonic()
        with self.lock:
            for key,maximum in ((('ip',peer),100),(('user',hashlib.sha256(email.encode()).hexdigest()),20)):
                rows=[x for x in self.limits.get(key,[]) if now-x<60]
                if len(rows)>=maximum:return False
                self.limits[key]=rows+[now]
            if len(self.limits)>10000:self.limits={k:v for k,v in self.limits.items() if v and now-v[-1]<60}
        return True
    def create_user(self,email,password):
        email=email.strip().casefold()
        if not (3<=len(email)<=254 and '@' in email and 12<=len(password)<=128):raise ValueError('invalid_account')
        salt=secrets.token_bytes(32);digest=hashlib.pbkdf2_hmac('sha256',password.encode(),salt,PASSWORD_ROUNDS);user=str(uuid.uuid4())
        with self.connect() as db:db.execute('INSERT INTO users VALUES(?,?,?,?)',(user,email,salt,digest))
        return user
    def verify_password(self,email,password):
        email=email.strip().casefold()
        with self.connect() as db:row=db.execute('SELECT id,salt,password FROM users WHERE email=?',(email,)).fetchone()
        # Unknown accounts still perform the same expensive password hash.
        salt=row[1] if row else bytes(32)
        supplied=hashlib.pbkdf2_hmac('sha256',password.encode(),salt,PASSWORD_ROUNDS)
        if not row or not hmac.compare_digest(supplied,row[2]):return None
        return row[0]
    def login(self,email,password):
        # Isolated integration fixtures only. The production handler never uses it.
        if not self.allow_registration:return None
        user=self.verify_password(email,password)
        if not user:return None
        token=secrets.token_hex(32);expires=int(time.time())+self.session_seconds
        with self.connect() as db:
            db.execute('DELETE FROM sessions WHERE expires<?',(int(time.time()),))
            db.execute('INSERT INTO sessions(digest,user_id,expires) VALUES(?,?,?)',(hashlib.sha256(token.encode()).hexdigest(),user,expires))
        return {'access_token':token,'expires_in':self.session_seconds,'user_id':user}
    def subject(self,token,require_app=True):
        if not self.allow_registration:return self.accounts.subject(token,require_app)
        if not HEX.fullmatch(token):return None
        with self.connect() as db:row=db.execute('SELECT user_id FROM sessions WHERE digest=? AND expires>?',(hashlib.sha256(token.encode()).hexdigest(),int(time.time()))).fetchone()
        return row[0] if row else None

class Handler(BaseHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def log_message(self,*args):pass # Never log credentials, personal identifiers or bundle paths.
    def handle(self):
        try:super().handle()
        except (ConnectionError,TimeoutError):pass # Normal client disconnect; never print peer identifiers.
    def setup(self):super().setup();self.connection.settimeout(15)
    def reply(self,status,value=None,body=None,content_type=None):
        raw=body if body is not None else json.dumps(value or {}).encode()
        self.send_response(status);self.send_header('Content-Type',content_type or ('application/octet-stream' if body is not None else 'application/json'));self.send_header('Content-Length',str(len(raw)));self.send_header('Cache-Control','no-store');self.send_header('X-Content-Type-Options','nosniff');self.end_headers();self.wfile.write(raw)
    def error(self,status,code):
        # A rejected request may still have an unread body: do not reuse its connection.
        self.close_connection=True
        self.reply(status,{'error':code})
    def read_body(self,maximum):
        if self.headers.get('Transfer-Encoding'):raise ValueError('unsupported_body')
        length=int(self.headers.get('Content-Length','0'))
        if not 0<length<=maximum:raise ValueError('invalid_body_size')
        raw=self.rfile.read(length)
        if len(raw)!=length:raise ValueError('incomplete_body')
        return raw
    def user(self):
        value=self.headers.get('Authorization','')
        return self.server.store.subject(value[7:],require_app=not (self.path.startswith('/v1/designs') or self.path in ('/v1/auth/account','/v1/auth/devices/revoke','/v1/auth/logout'))) if value.startswith('Bearer ') else None
    def do_POST(self):
        if self.path=='/v1/shopify/webhook':
            try:
                raw=self.read_body(2*1024*1024)
                self.server.store.accounts.webhook(raw,self.headers.get('X-Shopify-Hmac-Sha256',''),self.headers.get('X-Shopify-Shop-Domain',''),self.headers.get('X-Shopify-Topic',''),self.headers.get('X-Shopify-Webhook-Id',''))
                return self.reply(200,{'received':True})
            except (ValueError,TypeError,KeyError,AttributeError,OverflowError):return self.error(400,'webhook_rejected')
        if not self.server.store.allow_registration:
            return self.account_post()
        if self.path not in ('/v1/auth/login','/v1/auth/register'):return self.error(404,'not_found')
        try:
            body=json.loads(self.read_body(4096))
            if not isinstance(body,dict):return self.error(400,'invalid_request')
            email=body.get('email','');password=body.get('password','')
            if not isinstance(email,str) or not isinstance(password,str) or len(email)>254 or len(password)>128:return self.error(400,'invalid_account')
            if not self.server.store.rate(self.client_address[0],email.casefold()):return self.error(429,'rate_limited')
            if self.path.endswith('/register'):
                if not self.server.store.allow_registration:return self.error(403,'registration_disabled')
                try:self.server.store.create_user(email,password)
                except (ValueError,sqlite3.IntegrityError):return self.error(400,'account_not_created')
            result=self.server.store.login(email,password)
            if not result:return self.error(401,'invalid_credentials')
            return self.reply(200,result)
        except (ValueError,TypeError,json.JSONDecodeError):return self.error(400,'invalid_request')
    def account_post(self):
        if self.path=='/v1/auth/register':return self.error(403,'registration_disabled')
        accounts=self.server.store.accounts
        try:
            body=json.loads(self.read_body(4096))
            if not isinstance(body,dict):raise ValueError()
            email=body.get('email','')
            if not isinstance(email,str) or len(email)>254 or not self.server.store.rate(self.client_address[0],email.casefold()):return self.error(429,'rate_limited')
            if self.path=='/v1/auth/start':
                accounts.start(email,body.get('purpose','activate'));return self.reply(200,{'message':'Si la compra y la solicitud son válidas, recibirás un código.'})
            if self.path=='/v1/auth/activate':
                result=accounts.activate(email,body.get('code'),body.get('password'),body.get('password_confirmation'),body.get('device_id'),self.client_address[0]);return self.reply(200,result)
            if self.path=='/v1/auth/recover':
                return self.reply(200,accounts.recover(email,body.get('code'),body.get('password'),body.get('password_confirmation'),body.get('device_id'),body.get('reset_devices',False),self.client_address[0]))
            if self.path=='/v1/auth/login':
                password=body.get('password','')
                if not isinstance(password,str) or len(password)>128:raise ValueError()
                return self.reply(200,accounts.login(email,password,body.get('device_id'),self.client_address[0]))
            user=self.user()
            if not user:return self.error(401,'unauthorized')
            if self.path=='/v1/auth/devices/revoke':
                device=body.get('device_id','')
                if not isinstance(device,str) or not HEX.fullmatch(device):raise ValueError()
                accounts.revoke(user,device);return self.reply(200)
            if self.path=='/v1/auth/logout':
                token=self.headers.get('Authorization','')[7:]
                with self.server.store.connect() as db:db.execute('DELETE FROM sessions WHERE digest=?',(hashlib.sha256(token.encode()).hexdigest(),))
                return self.reply(200)
            return self.error(404,'not_found')
        except AccountError as error:return self.error(403,str(error))
        except (ValueError,TypeError,json.JSONDecodeError):return self.error(400,'invalid_request')
    def do_GET(self):
        if self.path=='/__fixture_redirect' and self.server.store.allow_registration:
            self.send_response(302);self.send_header('Location','/health');self.send_header('Content-Length','0');self.end_headers();return
        if self.path=='/health':return self.reply(200,{'status':'ok'})
        user=self.user()
        if not user:return self.error(401,'unauthorized')
        if self.path=='/v1/designs' or self.path.startswith('/v1/designs/'):
            if not self.server.store.accounts.pack_entitled(user):
                if self.path=='/v1/designs':return self.reply(200,{'schema':1,'pack_id':'studio-50-v1','entitled':False,'designs':[]})
                return self.error(403,'design_pack_required')
            try:
                catalog=self.server.store.design_catalog.catalog()
                if self.path=='/v1/designs':return self.reply(200,catalog_for_client(catalog,self.headers.get(EXPANDED_CATALOG_HEADER)))
                ident=self.path.removeprefix('/v1/designs/')
                thumbnail=ident.endswith('/thumbnail')
                if thumbnail:ident=ident.removesuffix('/thumbnail')
                item=next((item for item in catalog['designs'] if item['id']==ident),None)
                if not item:return self.error(404,'not_found')
                item=design_for_client(item,self.headers.get(EXPANDED_CATALOG_HEADER),thumbnail)
                return self.reply(200,body=self.server.store.design_catalog.read(item),content_type='image/png')
            except (ValueError,OSError,KeyError,RemoteError):return self.error(503,'design_catalog_unavailable')
        if self.path in ('/v1/auth/session','/v1/auth/account'):return self.reply(200,{'user_id':user,'app_licensed':self.server.store.allow_registration or self.server.store.accounts.app_entitled(user),'devices':self.server.store.accounts.devices(user)})
        if self.path=='/v1/bundles':
            with self.server.store.connect() as db:rows=db.execute('SELECT id,size FROM bundles WHERE user_id=? ORDER BY id LIMIT 1001',(user,)).fetchall()
            if len(rows)>1000:return self.error(409,'account_object_limit')
            return self.reply(200,{'bundles':[{'id':i,'sha256':i,'size':n} for i,n in rows]})
        ident=self.path.removeprefix('/v1/bundles/')
        if not self.path.startswith('/v1/bundles/') or not HEX.fullmatch(ident):return self.error(404,'not_found')
        with self.server.store.connect() as db:row=db.execute('SELECT size FROM bundles WHERE user_id=? AND id=?',(user,ident)).fetchone()
        if not row:return self.error(404,'not_found')
        store=self.server.store
        try:data=store.blob_store.read(user,ident) if store.remote else (store.blobs/user/ident).read_bytes()
        except (OSError,RemoteError):return self.error(503,'object_unavailable')
        if len(data)!=row[0] or hashlib.sha256(data).hexdigest()!=ident:return self.error(503,'object_integrity_error')
        return self.reply(200,body=data)
    def do_PUT(self):
        user=self.user()
        if not user:return self.error(401,'unauthorized')
        ident=self.path.removeprefix('/v1/bundles/')
        if not self.path.startswith('/v1/bundles/') or not HEX.fullmatch(ident):return self.error(400,'invalid_object_id')
        try:
            if int(self.headers.get('Content-Length','0'))>self.server.store.max_bundle:return self.error(413,'remote_bundle_size_limit' if self.server.store.remote else 'bundle_size_limit')
            data=self.read_body(self.server.store.max_bundle)
            if hashlib.sha256(data).hexdigest()!=ident or self.headers.get('X-Content-SHA256',ident)!=ident:return self.error(400,'object_digest_mismatch')
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                files=archive.infolist()
                if not 0<len(files)<=512 or sum(f.file_size for f in files)>256*1024*1024:return self.error(400,'invalid_bundle_limits')
                for f in files:
                    path=pathlib.PurePosixPath(f.filename)
                    if path.is_absolute() or '..' in path.parts or '\\' in f.filename or ':' in f.filename or f.flag_bits&1:return self.error(400,'invalid_bundle')
        except (ValueError,zipfile.BadZipFile):return self.error(400,'invalid_bundle')
        store=self.server.store
        with store.lock:
            with store.connect() as db:
                old=db.execute('SELECT size FROM bundles WHERE user_id=? AND id=?',(user,ident)).fetchone()
                if store.remote:
                    count,total=db.execute('SELECT count(*),coalesce(sum(size),0) FROM bundles WHERE user_id=?',(user,)).fetchone()
                    if not old and (count>=1000 or total+len(data)>1024*1024*1024):return self.error(413,'account_storage_limit')
                    try:store.blob_store.create(user,ident,data)
                    except RemoteError:return self.error(503,'immutable_object_not_saved')
                    if not old:db.execute('INSERT INTO bundles VALUES(?,?,?)',(user,ident,len(data)))
                    return self.reply(200 if old else 201,{'id':ident,'sha256':ident,'size':len(data)})
                directory=store.blobs/user;directory.mkdir(exist_ok=True)
                if old:
                    if (directory/ident).read_bytes()!=data:return self.error(409,'immutable_object_conflict')
                    return self.reply(200,{'id':ident,'sha256':ident,'size':len(data)})
                count,total=db.execute('SELECT count(*),coalesce(sum(size),0) FROM bundles WHERE user_id=?',(user,)).fetchone()
                if count>=1000 or total+len(data)>1024*1024*1024:return self.error(413,'account_storage_limit')
                temp=directory/(ident+'.'+secrets.token_hex(8)+'.pending')
                with temp.open('xb') as f:f.write(data);f.flush();os.fsync(f.fileno())
                destination=directory/ident
                if destination.exists():
                    if destination.read_bytes()!=data:return self.error(409,'immutable_object_conflict')
                    temp.unlink()
                else:temp.rename(destination)
                db.execute('INSERT INTO bundles VALUES(?,?,?)',(user,ident,len(data)))
        return self.reply(201,{'id':ident,'sha256':ident,'size':len(data)})

class Server(ThreadingHTTPServer):
    daemon_threads=True
    def __init__(self,address,store):
        maximum=os.environ.get('WALLET_MAX_CONCURRENT_REQUESTS','4')
        if maximum not in ('1','2','3','4'):raise ValueError('WALLET_MAX_CONCURRENT_REQUESTS must be an integer from 1 to 4')
        self.store=store;self.slots=threading.BoundedSemaphore(int(maximum));super().__init__(address,Handler)
    def process_request(self,request,address):
        if not self.slots.acquire(blocking=False):
            # Small bounded reply, no handler/thread or request-body allocation.
            try:
                body=b'{"error":"server_busy"}'
                request.settimeout(0.2)
                # Drain only bounded headers so closing a socket with an unread
                # GET does not reset/discard the 503 on Windows TCP stacks.
                header=bytearray();deadline=time.monotonic()+0.2
                while b'\r\n\r\n' not in header and len(header)<8192:
                    remaining=deadline-time.monotonic()
                    if remaining<=0:break
                    request.settimeout(remaining)
                    chunk=request.recv(8192-len(header))
                    if not chunk:break
                    header.extend(chunk)
                request.sendall(b'HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nRetry-After: 1\r\nCache-Control: no-store\r\nContent-Type: application/json\r\nContent-Length: '+str(len(body)).encode()+b'\r\n\r\n'+body)
                request.shutdown(1) # SHUT_WR: finish the bounded reply before close.
            except OSError:pass
            finally:request.close()
            return
        try:super().process_request(request,address)
        except BaseException:self.slots.release();raise
    def process_request_thread(self,request,address):
        try:super().process_request_thread(request,address)
        finally:self.slots.release()
    def handle_error(self,request,address):
        # socketserver's default dumps client IP + traceback. Keep production
        # errors observable without exposing request/account/backup details.
        import logging
        logging.error('request_failed')
def main():
    p=argparse.ArgumentParser();p.add_argument('--data',default=os.environ.get('WALLET_DATA','/data'));p.add_argument('--host',default='127.0.0.1');p.add_argument('--port',type=int,default=8766);p.add_argument('--create-user');args=p.parse_args()
    # Registration fixtures are never enabled through a production environment flag.
    try:store=Store(args.data)
    except Exception as error:
        failure=error if isinstance(error,StartupError) else StartupError('local_storage',error)
        print(failure.log_line(),file=sys.stderr,flush=True)
        p.error('No se pudo iniciar el almacenamiento privado. Revisa la configuración y disponibilidad; no se usará un almacén local alternativo.')
    if args.create_user:store.create_user(args.create_user,getpass.getpass('Contraseña de al menos 12 caracteres: '));print('Cuenta creada');return
    try:server=Server((args.host,args.port),store)
    except ValueError as error:p.error(str(error))
    server.serve_forever()
if __name__=='__main__':main()
