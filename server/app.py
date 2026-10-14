"""Private immutable backup service. Python standard library; HTTPS proxy in production."""
import argparse,contextlib,hashlib,hmac,json,os,pathlib,re,secrets,sqlite3,threading,time,uuid,zipfile,io,getpass
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer

HEX=re.compile(r'^[0-9a-f]{64}$')
MAX_BUNDLE=100*1024*1024
PASSWORD_ROUNDS=600_000
class Store:
    def __init__(self,root,allow_registration=False,session_seconds=86400*7):
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
    @contextlib.contextmanager
    def connect(self):
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
    def login(self,email,password):
        email=email.strip().casefold()
        with self.connect() as db:row=db.execute('SELECT id,salt,password FROM users WHERE email=?',(email,)).fetchone()
        # Unknown accounts still perform the same expensive password hash.
        salt=row[1] if row else bytes(32)
        supplied=hashlib.pbkdf2_hmac('sha256',password.encode(),salt,PASSWORD_ROUNDS)
        if not row or not hmac.compare_digest(supplied,row[2]):return None
        token=secrets.token_hex(32);expires=int(time.time())+self.session_seconds
        with self.connect() as db:
            db.execute('DELETE FROM sessions WHERE expires<?',(int(time.time()),))
            db.execute('INSERT INTO sessions VALUES(?,?,?)',(hashlib.sha256(token.encode()).hexdigest(),row[0],expires))
        return {'access_token':token,'expires_in':self.session_seconds,'user_id':row[0]}
    def subject(self,token):
        if not HEX.fullmatch(token):return None
        with self.connect() as db:row=db.execute('SELECT user_id FROM sessions WHERE digest=? AND expires>?',(hashlib.sha256(token.encode()).hexdigest(),int(time.time()))).fetchone()
        return row[0] if row else None

class Handler(BaseHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def log_message(self,*args):pass # Never log credentials, personal identifiers or bundle paths.
    def setup(self):super().setup();self.connection.settimeout(15)
    def reply(self,status,value=None,body=None):
        raw=body if body is not None else json.dumps(value or {}).encode()
        self.send_response(status);self.send_header('Content-Type','application/octet-stream' if body is not None else 'application/json');self.send_header('Content-Length',str(len(raw)));self.send_header('Cache-Control','no-store');self.send_header('X-Content-Type-Options','nosniff');self.end_headers();self.wfile.write(raw)
    def error(self,status,code):self.reply(status,{'error':code})
    def read_body(self,maximum):
        if self.headers.get('Transfer-Encoding'):raise ValueError('unsupported_body')
        length=int(self.headers.get('Content-Length','0'))
        if not 0<length<=maximum:raise ValueError('invalid_body_size')
        raw=self.rfile.read(length)
        if len(raw)!=length:raise ValueError('incomplete_body')
        return raw
    def user(self):
        value=self.headers.get('Authorization','')
        return self.server.store.subject(value[7:]) if value.startswith('Bearer ') else None
    def do_POST(self):
        if self.path not in ('/v1/auth/login','/v1/auth/register'):return self.error(404,'not_found')
        try:
            body=json.loads(self.read_body(4096));email=body.get('email','');password=body.get('password','')
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
    def do_GET(self):
        if self.path=='/health':return self.reply(200,{'status':'ok'})
        user=self.user()
        if not user:return self.error(401,'unauthorized')
        if self.path=='/v1/bundles':
            with self.server.store.connect() as db:rows=db.execute('SELECT id,size FROM bundles WHERE user_id=? ORDER BY id LIMIT 1001',(user,)).fetchall()
            if len(rows)>1000:return self.error(409,'account_object_limit')
            return self.reply(200,{'bundles':[{'id':i,'sha256':i,'size':n} for i,n in rows]})
        ident=self.path.removeprefix('/v1/bundles/')
        if not self.path.startswith('/v1/bundles/') or not HEX.fullmatch(ident):return self.error(404,'not_found')
        with self.server.store.connect() as db:row=db.execute('SELECT size FROM bundles WHERE user_id=? AND id=?',(user,ident)).fetchone()
        if not row:return self.error(404,'not_found')
        path=self.server.store.blobs/user/ident
        try:data=path.read_bytes()
        except OSError:return self.error(503,'object_unavailable')
        if len(data)!=row[0] or hashlib.sha256(data).hexdigest()!=ident:return self.error(503,'object_integrity_error')
        return self.reply(200,body=data)
    def do_PUT(self):
        user=self.user()
        if not user:return self.error(401,'unauthorized')
        ident=self.path.removeprefix('/v1/bundles/')
        if not self.path.startswith('/v1/bundles/') or not HEX.fullmatch(ident):return self.error(400,'invalid_object_id')
        try:
            data=self.read_body(MAX_BUNDLE)
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
    def __init__(self,address,store):self.store=store;self.slots=threading.BoundedSemaphore(32);super().__init__(address,Handler)
    def process_request(self,request,address):
        if not self.slots.acquire(blocking=False):request.close();return
        try:super().process_request(request,address)
        except BaseException:self.slots.release();raise
    def process_request_thread(self,request,address):
        try:super().process_request_thread(request,address)
        finally:self.slots.release()
def main():
    p=argparse.ArgumentParser();p.add_argument('--data',default=os.environ.get('WALLET_DATA','/data'));p.add_argument('--host',default='127.0.0.1');p.add_argument('--port',type=int,default=8766);p.add_argument('--create-user');args=p.parse_args()
    store=Store(args.data,os.environ.get('WALLET_ALLOW_REGISTRATION')=='1')
    if args.create_user:store.create_user(args.create_user,getpass.getpass('Contraseña de al menos 12 caracteres: '));print('Cuenta creada');return
    Server((args.host,args.port),store).serve_forever()
if __name__=='__main__':main()
