"""Service-only PostgreSQL and immutable private Supabase objects.

All SQL transactions take one advisory xact lock. This intentionally serialises
this small service across processes, including credential rotation and OTPs.
No session locks, search_path state or prepared statements (transaction pooler).
"""
import contextlib,hashlib,json,os,re,ssl,threading,urllib.request,urllib.error,urllib.parse

MAX_REMOTE_BUNDLE=32*1024*1024
TABLES=('users','sessions','bundles','purchase_orders','webhook_events','activation','computers')
SCHEMA='wallet_private'
LOCK_ID=6299748183224241

class RemoteError(ValueError):pass
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*args,**kwargs):return None

def https_request(url,method,headers,data=None,maximum=MAX_REMOTE_BUNDLE):
    opener=urllib.request.build_opener(NoRedirect,urllib.request.HTTPSHandler(context=ssl.create_default_context()))
    try:
        with opener.open(urllib.request.Request(url,data=data,headers=headers,method=method),timeout=20) as response:
            result=response.read(maximum+1)
            if len(result)>maximum:raise RemoteError('remote_response_too_large')
            return response.status,result
    except urllib.error.HTTPError as error:
        # Do not propagate URLs, keys, response bodies or personal data to logs.
        return error.code,b''
    except (OSError,ValueError):raise RemoteError('remote_service_unavailable') from None

def sql_for_postgres(sql):
    if sql.strip().upper().startswith('PRAGMA') or sql.strip().upper()=='BEGIN IMMEDIATE':return None
    replace=re.match(r'INSERT OR REPLACE INTO (activation|purchase_orders) VALUES',sql)
    if replace:
        table=replace[1];sql=sql.replace('INSERT OR REPLACE','INSERT',1)
        key='email' if table=='activation' else 'id'
        fields=('digest','expires','attempts','last_sent') if table=='activation' else ('email','active','updated')
        sql+=' ON CONFLICT('+key+') DO UPDATE SET '+','.join(f'{field}=excluded.{field}' for field in fields)
    pieces=re.split(r"('(?:''|[^'])*')",sql)
    for index in range(0,len(pieces),2):
        text=pieces[index].replace(' BLOB',' BYTEA').replace(' INTEGER',' BIGINT').replace(' REAL',' DOUBLE PRECISION').replace('?','%s')
        pieces[index]=re.sub(r'\b('+('|'.join(TABLES))+r')\b',lambda match:SCHEMA+'.'+match[0],text)
    return ''.join(pieces)

class PgAdapter:
    def __init__(self,connection):self.connection=connection
    def execute(self,sql,parameters=()):
        sql=sql_for_postgres(sql)
        return self.connection.execute(sql,parameters) if sql else None
    def executescript(self,sql):
        for statement in sql.split(';'):
            if statement.strip():self.execute(statement)

class PgLock:
    def __init__(self,database):self.database=database;self.local=threading.local();self.guard=threading.RLock()
    def __enter__(self):
        self.guard.acquire()
        try:
            depth=getattr(self.local,'depth',0)
            if depth==0:
                connection=self.database.open();self.local.connection=connection
                connection.execute('SELECT pg_advisory_xact_lock(%s)',(LOCK_ID,))
            self.local.depth=depth+1;return self
        except BaseException:
            if getattr(self.local,'connection',None):self.local.connection.close();self.local.connection=None
            self.guard.release();raise
    def __exit__(self,kind,value,trace):
        try:
            self.local.depth-=1
            if self.local.depth==0:
                connection=self.local.connection
                try:
                    # AccountError intentionally commits earlier successful
                    # savepoints (failed OTP counters), never failed savepoints.
                    if kind is None or kind.__name__=='AccountError':connection.commit()
                    else:connection.rollback()
                finally:connection.close();self.local.connection=None
        finally:self.guard.release()

class PgDatabase:
    def __init__(self,url,connector=None):
        self.url=url;self.connector=connector;self.lock=PgLock(self)
    def open(self):
        connector=self.connector
        if connector is None:
            import psycopg
            connector=psycopg.connect
        # Debian's CA bundle validates both chain and hostname. No plaintext or
        # sslmode=require fallback is permitted, including URL query overrides.
        try:return connector(self.url,prepare_threshold=None,sslmode='verify-full',sslrootcert=os.environ.get('DATABASE_SSL_ROOT_CERT','/etc/ssl/certs/ca-certificates.crt'),connect_timeout=10)
        except Exception:raise RemoteError('database_unavailable') from None
    @contextlib.contextmanager
    def connect(self):
        with self.lock:
            connection=self.lock.local.connection
            with connection.transaction():yield PgAdapter(connection)
    def initialize(self):
        with self.connect() as db:
            db.connection.execute('CREATE SCHEMA IF NOT EXISTS wallet_private')
            db.connection.execute('REVOKE ALL ON SCHEMA wallet_private FROM PUBLIC')
            # Supabase-managed roles exist in production; keep vanilla Postgres
            # integration possible without creating public access roles.
            for role in ('anon','authenticated'):
                if db.connection.execute('SELECT 1 FROM pg_roles WHERE rolname=%s',(role,)).fetchone():
                    db.connection.execute(f'REVOKE ALL ON SCHEMA wallet_private FROM {role}')
            db.executescript('''CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,email TEXT UNIQUE NOT NULL,salt BLOB NOT NULL,password BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS sessions(digest TEXT PRIMARY KEY,user_id TEXT NOT NULL,expires INTEGER NOT NULL,device_id TEXT NOT NULL DEFAULT '',last_seen INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS bundles(user_id TEXT NOT NULL,id TEXT NOT NULL,size INTEGER NOT NULL,PRIMARY KEY(user_id,id));''')
    def secure_tables(self):
        with self.connect() as db:
            for table in TABLES:
                db.connection.execute(f'ALTER TABLE wallet_private.{table} ENABLE ROW LEVEL SECURITY')
                db.connection.execute(f'REVOKE ALL ON TABLE wallet_private.{table} FROM PUBLIC')
                for role in ('anon','authenticated'):
                    if db.connection.execute('SELECT 1 FROM pg_roles WHERE rolname=%s',(role,)).fetchone():db.connection.execute(f'REVOKE ALL ON TABLE wallet_private.{table} FROM {role}')

class SupabaseBlobs:
    def __init__(self,url,key,bucket,request=https_request):
        parsed=urllib.parse.urlsplit(url)
        if parsed.scheme!='https' or not parsed.hostname or not parsed.hostname.endswith('.supabase.co') or parsed.username or parsed.password or parsed.path not in ('','/') or parsed.query or parsed.fragment or parsed.port not in (None,443):raise RemoteError('invalid_supabase_url')
        if not key or any(c.isspace() for c in key) or not re.fullmatch('[a-z0-9][a-z0-9-]{0,62}',bucket):raise RemoteError('invalid_storage_configuration')
        self.base=url.rstrip('/')+'/storage/v1';self.bucket=bucket;self.request=request
        self.headers={'apikey':key,'Authorization':'Bearer '+key}
    def verify_bucket(self):
        status,body=self.request(self.base+'/bucket/'+self.bucket,'GET',self.headers,maximum=65536)
        if status==404:
            data=json.dumps({'id':self.bucket,'name':self.bucket,'public':False,'file_size_limit':MAX_REMOTE_BUNDLE,'allowed_mime_types':['application/zip']}).encode()
            status,_=self.request(self.base+'/bucket','POST',dict(self.headers,**{'Content-Type':'application/json'}),data,65536)
            if status not in (200,201,400,409):raise RemoteError('private_bucket_not_created')
            status,body=self.request(self.base+'/bucket/'+self.bucket,'GET',self.headers,maximum=65536)
        if status!=200 or json.loads(body).get('public') is not False:raise RemoteError('storage_bucket_must_be_private')
    def path(self,user,ident):
        if not re.fullmatch('[a-zA-Z0-9-]{1,64}',user) or not re.fullmatch('[0-9a-f]{64}',ident):raise RemoteError('invalid_object_key')
        return self.bucket+'/'+user+'/'+ident
    def read(self,user,ident):
        status,data=self.request(self.base+'/object/authenticated/'+self.path(user,ident),'GET',self.headers)
        if status!=200 or hashlib.sha256(data).hexdigest()!=ident:raise RemoteError('object_unavailable_or_corrupt')
        return data
    def create(self,user,ident,data):
        if len(data)>MAX_REMOTE_BUNDLE or hashlib.sha256(data).hexdigest()!=ident:raise RemoteError('invalid_remote_object')
        status,_=self.request(self.base+'/object/'+self.path(user,ident),'POST',dict(self.headers,**{'Content-Type':'application/zip','x-upsert':'false'}),data,65536)
        if status not in (200,201,400,409):raise RemoteError('object_not_saved')
        # An orphan from an interrupted DB commit may already exist. Accept it
        # only after authenticated readback proves exactly the expected bytes.
        if self.read(user,ident)!=data:raise RemoteError('immutable_object_conflict')
