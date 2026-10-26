"""Synthetic R2/private preflight checks. Never connects to a provider or real DB."""
import contextlib,copy,datetime,hashlib,hmac,io,json,os,pathlib,sys,unittest,urllib.parse
from unittest.mock import patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from remote import R2Blobs,SupabaseBlobs,RemoteError,configured_blobs,MAX_REMOTE_BUNDLE
import migrate_storage as migration
from designs import DesignCatalog,object_key
from test_native_catalog import png_at,native_metadata
import app

def config():return {'R2_ACCOUNT_ID':'a'*32,'R2_ACCESS_KEY_ID':'synthetic-access','R2_SECRET_ACCESS_KEY':'synthetic-secret','CLOUDFLARE_API_TOKEN':'synthetic-control','R2_BACKUP_BUCKET':'wallet-backups','R2_DESIGN_BUCKET':'wallet-designs','WALLET_OBJECT_STORAGE':'r2'}

class R2Tests(unittest.TestCase):
 def test_remote_store_explicit_r2_selection_keeps_database_and_private_bucket_checks(self):
  cfg=dict(config(),WALLET_STORAGE_MODE='remote',DATABASE_URL='postgresql://synthetic-target')
  with patch.dict(os.environ,cfg,clear=True),patch('app.PgDatabase') as database,patch('app.Accounts'),patch('app.configured_blobs') as factory:
   store=app.Store('unused-synthetic-path');self.assertTrue(store.remote);self.assertIs(store.blob_store,factory.return_value)
   factory.assert_called_once();factory.return_value.verify_bucket.assert_called_once();database.return_value.initialize.assert_called_once();database.return_value.secure_tables.assert_called_once()
 def test_conditional_put_signs_payload_and_readback_is_exact(self):
  cfg=config();data=b'synthetic immutable archive';ident=hashlib.sha256(data).hexdigest();calls=[]
  def request(url,method,headers,body=None,maximum=0):
   calls.append((url,method,headers,body,maximum))
   self.assertTrue(url.startswith('https://'+'a'*32+'.r2.cloudflarestorage.com/'));self.assertNotIn('synthetic-control',str(headers))
   return (200,b'') if method=='PUT' else (200,data)
  service=R2Blobs(cfg,'wallet-backups',request,lambda:datetime.datetime(2026,10,26,tzinfo=datetime.timezone.utc));service.create('synthetic-user',ident,data)
  self.assertEqual([call[1] for call in calls],['PUT','GET']);headers=calls[0][2];self.assertEqual(headers['if-none-match'],'*');self.assertEqual(calls[0][4],65536)
  payload=hashlib.sha256(data).hexdigest();host='a'*32+'.r2.cloudflarestorage.com';uri='/wallet-backups/synthetic-user/'+ident
  names='content-type;host;if-none-match;x-amz-content-sha256;x-amz-date'
  canonical=f'PUT\n{uri}\n\ncontent-type:application/zip\nhost:{host}\nif-none-match:*\nx-amz-content-sha256:{payload}\nx-amz-date:20261026T000000Z\n\n{names}\n{payload}'
  signing=b'AWS4synthetic-secret'
  for part in [b'20261026',b'auto',b's3',b'aws4_request']:signing=hmac.digest(signing,part,'sha256')
  message='AWS4-HMAC-SHA256\n20261026T000000Z\n20261026/auto/s3/aws4_request\n'+hashlib.sha256(canonical.encode()).hexdigest()
  expected=hmac.digest(signing,message.encode(),'sha256').hex()
  self.assertEqual(headers['Authorization'],f'AWS4-HMAC-SHA256 Credential=synthetic-access/20261026/auto/s3/aws4_request, SignedHeaders={names}, Signature={expected}')
  self.assertNotIn('synthetic-secret',str(calls))
 def test_existing_conflict_never_overwrites_and_requires_readback(self):
  data=b'expected';ident=hashlib.sha256(data).hexdigest()
  for status,existing,success in [(412,data,True),(409,data,True),(412,b'occupied',False),(403,data,False),(302,data,False)]:
   calls=[]
   def request(url,method,headers,body=None,maximum=0):
    calls.append(method)
    if method=='PUT':self.assertEqual(headers['if-none-match'],'*');return status,b''
    return 200,existing
   service=R2Blobs(config(),'wallet-backups',request)
   if success:service.create('synthetic',ident,data)
   else:
    with self.assertRaises(RemoteError):service.create('synthetic',ident,data)
   self.assertEqual(calls,['PUT','GET'] if status in (409,412) else ['PUT'])
 def test_private_bucket_requires_both_control_endpoints_and_separates_credentials(self):
  for managed,custom,success in [(False,[],True),(True,[],False),(False,[{'enabled':True}],False),(False,[{'enabled':False}],True),(None,[],False),(False,[{}],False)]:
   calls=[]
   def request(url,method,headers,body=None,maximum=0):
    calls.append(method);self.assertEqual(method,'GET' if 'api.cloudflare.com/' in url else 'HEAD')
    if 'api.cloudflare.com/' in url:
     self.assertEqual(headers,{'Authorization':'Bearer synthetic-control'});self.assertEqual(maximum,65536)
     return 200,json.dumps({'success':True,'result':{'enabled':managed} if url.endswith('/managed') else {'domains':custom}}).encode()
    self.assertIn('Credential=synthetic-access/',headers['Authorization']);self.assertNotIn('synthetic-control',str(headers));return 200,b''
   service=R2Blobs(config(),'wallet-backups',request)
   if success:service.verify_bucket();self.assertEqual(calls,['GET','GET','HEAD'])
   else:
    with self.assertRaises(RemoteError):service.verify_bucket()
    self.assertNotIn('HEAD',calls)
  for response in [(403,b''),(200,b'bad'),(200,b'{"success":false,"result":{}}')]:
   with self.assertRaises(RemoteError):R2Blobs(config(),'wallet-backups',lambda *a,**k:response).verify_bucket()
 def test_invalid_endpoints_keys_and_limits_fail_before_network(self):
  for field,value in [('R2_ACCOUNT_ID','https://evil.test'),('R2_ACCOUNT_ID','A'*32),('R2_ACCESS_KEY_ID','bad\nkey'),('CLOUDFLARE_API_TOKEN','')]:
   bad=dict(config(),**{field:value})
   with self.assertRaises(RemoteError):R2Blobs(bad,'wallet-backups')
  calls=[];service=R2Blobs(config(),'wallet-backups',lambda *a,**k:calls.append(a))
  for key in ['../private','owner/../file','/absolute','owner//file','owner/a?secret','owner/back\\slash']:
   with self.assertRaises(RemoteError):service.read_key(key,10)
  for limit in [0,MAX_REMOTE_BUNDLE+1,True]:
   with self.assertRaises(RemoteError):service.read_key('owner/object',limit)
  self.assertEqual(calls,[])
 def test_provider_default_stays_supabase_and_design_r2_bytes_remain_native(self):
  legacy={'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'synthetic-service','SUPABASE_STORAGE_BUCKET':'backups'}
  self.assertIsInstance(configured_blobs(legacy),SupabaseBlobs);self.assertIsInstance(configured_blobs(config()),R2Blobs)
  with self.assertRaises(RemoteError):configured_blobs(dict(config(),WALLET_OBJECT_STORAGE='unknown'))
  data=png_at(1536,969);item=native_metadata(data)[0];calls=[]
  def request(url,method,headers,body=None,maximum=0):
   calls.append(method)
   if 'api.cloudflare.com/' in url:return 200,json.dumps({'success':True,'result':{'enabled':False} if url.endswith('/managed') else {'domains':[]}}).encode()
   if method=='HEAD':return 200,b''
   self.assertTrue(url.endswith('/wallet-designs/'+object_key(item)));self.assertEqual(maximum,4*1024*1024);return 200,data
  service=DesignCatalog(config());service.blobs=lambda:R2Blobs(config(),'wallet-designs',request)
  self.assertEqual(service.read(item),data);self.assertEqual(calls,['GET','GET','HEAD','GET']);service.read(item);self.assertEqual(len(calls),4)

class FakeDB:
 def __init__(self,empty=False,invalid=None):self.empty=empty;self.invalid=invalid;self.calls=[];self.closed=False
 def transaction(self):return contextlib.nullcontext()
 def close(self):self.closed=True
 def execute(self,sql,args=()):
  self.calls.append(sql)
  if sql.startswith('SET '):rows=[]
  elif 'FROM pg_tables' in sql:rows=[] if self.empty else [(name,) for name in migration.TABLES]
  elif 'information_schema.columns' in sql:rows=[(table,name,kind,'NO') for table,fields in sorted(migration.FIELDS.items()) for name,kind in fields]
  elif 'SELECT t.relname,c.contype' in sql:rows=[(table,'p',fields) for table,fields in migration.PRIMARY.items()]+[('users','u',('email',))]
  elif 'FROM pg_index x' in sql:rows=[('purchase_orders','purchase_email',('email','active'),True),('purchase_packs','purchase_pack_email',('email','active'),True)]
  elif 'SELECT NOT EXISTS' in sql:rows=[(self.invalid!='privacy',)]
  elif sql.startswith('SELECT count(*)'):rows=[(1,)]
  else:raise AssertionError('unexpected SQL')
  if self.invalid=='columns' and 'information_schema' in sql:rows=rows[:-1]
  if self.invalid=='constraints' and 'c.contype' in sql:rows=rows[:-1]
  if self.invalid=='indexes' and 'FROM pg_index' in sql:rows=rows[:-1]
  class Result:
   def fetchall(self):return rows
   def fetchone(self):return rows[0]
  return Result()

class PreflightTests(unittest.TestCase):
 def test_paginated_inventory_includes_unindexed_objects_without_fetching_payloads(self):
  calls=[]
  def request(url,method,headers,body=None,maximum=0):
   calls.append(method)
   if '/bucket/' in url:return 200,b'{"public":false}'
   self.assertEqual(method,'POST');self.assertIn('/object/list/',url);value=json.loads(body)
   if value['prefix']=='':page=[{'name':'synthetic-owner','id':None,'metadata':None}]
   else:
    self.assertEqual(value['prefix'],'synthetic-owner/');page=[{'name':f'{index:064x}','id':'synthetic','metadata':{'size':123}} for index in range(value['offset'],min(value['offset']+100,101))]
   return 200,json.dumps(page).encode()
  blobs=SupabaseBlobs('https://fixture.supabase.co','synthetic','backups',request)
  objects=migration.inventory_supabase(blobs);self.assertEqual(len(objects),101);self.assertEqual(sum(objects.values()),101*123);self.assertEqual(calls,['GET','POST','POST','POST'])
 def test_unknown_keys_duplicate_pages_public_or_missing_bucket_fail_closed(self):
  for entry in [{'name':'unexpected','id':'x','metadata':{'size':12}},{'name':'0'*64,'id':'x','metadata':{'size':True}}]:
   def request(url,method,headers,body=None,maximum=0):return (200,b'{"public":false}') if '/bucket/' in url else (200,json.dumps([entry]).encode())
   with self.assertRaises(migration.MigrationError):migration.inventory_supabase(SupabaseBlobs('https://fixture.supabase.co','synthetic','backups',request))
  for status,body in [(404,b'{}'),(200,b'{"public":true}'),(200,b'bad')]:
   calls=[]
   def request(*args,**kwargs):calls.append(args[1]);return status,body
   with self.assertRaises(migration.MigrationError):migration.inventory_supabase(SupabaseBlobs('https://fixture.supabase.co','synthetic','backups',request))
   self.assertEqual(calls,['GET'])
 def test_database_schema_privacy_and_empty_target_are_checked_without_mutations(self):
  source=FakeDB();self.assertEqual(migration.inspect_database(source),{'tables':8,'rows':8,'empty':False})
  self.assertTrue(all(sql.startswith('SELECT') for sql in source.calls));self.assertEqual(migration.inspect_database(FakeDB(empty=True),target=True),{'tables':0,'rows':0,'empty':True})
  for bad in ['columns','constraints','indexes','privacy']:
   with self.assertRaises(migration.MigrationError):migration.inspect_database(FakeDB(invalid=bad))
  with self.assertRaises(migration.MigrationError):migration.inspect_database(FakeDB(),target=True)
 def test_preflight_uses_read_only_transactions_counts_only_and_commit_never_runs(self):
  cfg=dict(config(),MIGRATION_SOURCE_DATABASE_URL='postgresql://synthetic-source',MIGRATION_TARGET_DATABASE_URL='postgresql://synthetic-target',MIGRATION_SOURCE_SUPABASE_URL='https://fixture.supabase.co',MIGRATION_SOURCE_SUPABASE_KEY='synthetic-source-key',MIGRATION_SOURCE_BACKUP_BUCKET='source-backups')
  connections=[];methods=[]
  def connector(url,**kwargs):
   self.assertEqual(kwargs['sslmode'],'verify-full');db=FakeDB(empty=url.endswith('target'));connections.append(db);return db
  def request(url,method,headers,body=None,maximum=0):
   methods.append(method)
   if 'api.cloudflare.com/' in url:return 200,json.dumps({'success':True,'result':{'enabled':False} if url.endswith('/managed') else {'domains':[]}}).encode()
   if method=='HEAD':return 200,b''
   return (200,b'{"public":false}') if '/bucket/' in url else (200,b'[]')
  result=migration.preflight(cfg,connector=connector,request=request)
  self.assertEqual(result['result'],'preflight_only');self.assertTrue(result['commit_available']);self.assertTrue(result['source_unchanged'])
  self.assertEqual(result['backup_inventory'],{'objects':0,'bytes':0});self.assertNotIn('PUT',methods)
  self.assertTrue(all(connection.closed and connection.calls[0]=='SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY' for connection in connections))
  output=io.StringIO()
  with patch('sys.stdout',output),patch.object(migration,'preflight',side_effect=AssertionError('must not run')):self.assertEqual(migration.main(['--commit'],cfg),1)
  self.assertEqual(json.loads(output.getvalue()),{'result':'blocked','category':'source_writes_not_fenced'})

class MemoryDB(FakeDB):
 def __init__(self,rows=None,fail_table=None,bad_readback=False):
  super().__init__(empty=rows is None);self.rows=copy.deepcopy(rows or {table:[] for table in migration.TABLES});self.fail_table=fail_table;self.bad_readback=bad_readback;self.rollbacks=0;self.commits=0
 @contextlib.contextmanager
 def transaction(self):
  original=copy.deepcopy(self.rows);was_empty=self.empty
  try:yield
  except BaseException:self.rows=original;self.empty=was_empty;self.rollbacks+=1;raise
  else:self.commits+=1
 def result(self,rows):
  class Result:
   def fetchone(self):return rows[0] if rows else None
   def fetchall(self):return rows
  return Result()
 def execute(self,sql,args=()):
  if sql.startswith(('CREATE ','ALTER ','REVOKE ')):
   self.calls.append(sql)
   if sql.startswith('CREATE TABLE'):self.empty=False
   return self.result([])
  if sql.startswith('INSERT INTO '):
   self.calls.append(sql);table=sql.split('wallet_private.')[1].split('(')[0]
   if table==self.fail_table:raise RuntimeError('synthetic interrupted insert')
   self.rows[table].append(tuple(args));return self.result([])
  if 'column_default' in sql:
   self.calls.append(sql);return self.result([(table,name,"''::text" if (table,name)==('sessions','device_id') else '0' if (table,name)==('sessions','last_seen') else None) for table,fields in sorted(migration.FIELDS.items()) for name,_ in fields])
  if 'pg_policies' in sql or 'pg_trigger' in sql or 'pg_advisory_xact_lock' in sql:
   self.calls.append(sql);return self.result([(0,)])
  if 'FROM pg_roles' in sql:self.calls.append(sql);return self.result([(1,)])
  if sql.startswith('SELECT count(*) FROM wallet_private.') and 'LEFT JOIN' in sql:
   self.calls.append(sql);table=sql.split('FROM wallet_private.')[1].split(' ')[0];users={row[0] for row in self.rows['users']};computers={(row[0],row[1]) for row in self.rows['computers']}
   bad=sum((row[1] not in users) if table=='sessions' and 'computers c' not in sql else ((row[1],row[3]) not in computers and row[3]!='') if table=='sessions' else row[0] not in users for row in self.rows[table]);return self.result([(bad,)])
  if sql.startswith('SELECT count(*) FROM wallet_private.'):
   self.calls.append(sql);return self.result([(len(self.rows[sql.rsplit('.',1)[1]]),)])
  return super().execute(sql,args)
 @contextlib.contextmanager
 def cursor(self,name):
  database=self
  class Cursor:
   def execute(self,sql):
    database.calls.append(sql);table=sql.split('FROM wallet_private.')[1].split(' ')[0];fields=[name for name,_ in migration.FIELDS[table]]
    self.rows=sorted(database.rows[table],key=lambda row:tuple(row[fields.index(key)] for key in migration.PRIMARY[table]));self.position=0
    if database.bad_readback and table=='users' and self.rows:self.rows=[tuple(list(self.rows[0][:-1])+[b'changed readback'])]+self.rows[1:]
   def fetchmany(self,count):batch=self.rows[self.position:self.position+count];self.position+=len(batch);return batch
  yield Cursor()

class MigrationCommitTests(unittest.TestCase):
 def fixture(self,**target_options):
  indexed=b'synthetic indexed ZIP';orphan=b'synthetic orphan ZIP';ident=hashlib.sha256(indexed).hexdigest();orphan_id=hashlib.sha256(orphan).hexdigest()
  png=png_at(1536,969);item=native_metadata(png)[0]
  rows={'users':[('synthetic-owner','buyer@example.test',b'exact-salt',b'exact-password-hash')],
   'sessions':[('exact-session-digest','synthetic-owner',4102444800,'synthetic-device',1793000000)],
   'bundles':[('synthetic-owner',ident,len(indexed))],
   'purchase_orders':[('exact-order','buyer@example.test',1,1793000000.0)],
   'purchase_packs':[('exact-pack','buyer@example.test',1,1793000000.0)],
   'webhook_events':[('exact-webhook',1793000000)],
   'activation':[('buyer@example.test','exact-otp-digest',4102444800,1,1793000000)],
   'computers':[('synthetic-owner','synthetic-device',1793000000,1793000000,'exact-peer-digest')]}
  source=MemoryDB(rows);target=MemoryDB(**target_options)
  originals={('source-backups','synthetic-owner/'+ident):indexed,('source-backups','synthetic-owner/'+orphan_id):orphan,('wallet-designs',object_key(item)):png};destination={};calls=[]
  cfg=dict(config(),MIGRATION_SOURCE_DATABASE_URL='postgresql://synthetic-source',MIGRATION_TARGET_DATABASE_URL='postgresql://synthetic-target',MIGRATION_SOURCE_SUPABASE_URL='https://fixture.supabase.co',MIGRATION_SOURCE_SUPABASE_KEY='synthetic-source-key',MIGRATION_SOURCE_BACKUP_BUCKET='source-backups',MIGRATION_SOURCE_WRITES_PAUSED='confirmed')
  def connector(url,**kwargs):return target if url.endswith('target') else source
  def request(url,method,headers,body=None,maximum=0):
   calls.append((url,method))
   if 'api.cloudflare.com/' in url:return 200,json.dumps({'success':True,'result':{'enabled':False} if url.endswith('/managed') else {'domains':[]}}).encode()
   if 'fixture.supabase.co/' in url:
    if '/bucket/' in url:return 200,b'{"public":false}'
    if '/object/list/' in url:
     bucket=url.rsplit('/',1)[1];query=json.loads(body);prefix=query['prefix'];entries={}
     for (known_bucket,key),data in originals.items():
      if known_bucket!=bucket or not key.startswith(prefix):continue
      tail=key[len(prefix):];name=tail.split('/')[0]
      entries[name]={'name':name,'id':None,'metadata':None} if '/' in tail else {'name':name,'id':'synthetic','metadata':{'size':len(data)}}
     page=[entries[name] for name in sorted(entries)][query['offset']:query['offset']+query['limit']];return 200,json.dumps(page).encode()
    path=url.split('/object/authenticated/')[1];bucket,key=path.split('/',1);return 200,originals[(bucket,key)]
   if method=='HEAD':return 200,b''
   bucket,key=urllib.parse.urlsplit(url).path[1:].split('/',1)
   if method=='PUT':
    self.assertEqual(headers['if-none-match'],'*')
    if (bucket,key) in destination:return 412,b''
    destination[(bucket,key)]=body;return 200,b''
   return (200,destination[(bucket,key)]) if (bucket,key) in destination else (404,b'')
  return cfg,source,target,originals,destination,calls,connector,request,[{'designs':[item]}]
 def test_full_fenced_copy_preserves_every_row_object_orphan_and_source(self):
  cfg,source,target,objects,destination,calls,connector,request,catalogs=self.fixture();rows=copy.deepcopy(source.rows);originals=dict(objects)
  result=migration.commit_migration(cfg,connector=connector,request=request,catalogs=catalogs)
  self.assertEqual(result['result'],'copied_not_activated');self.assertEqual(result['database'],{'tables':8,'rows':8,'exact_readback':True});self.assertEqual(result['backup_objects']['objects'],2)
  self.assertEqual(source.rows,rows);self.assertEqual(objects,originals);self.assertEqual(target.rows,rows)
  expected={(('wallet-backups' if bucket=='source-backups' else 'wallet-designs'),key):data for (bucket,key),data in objects.items()};self.assertEqual(destination,expected)
  self.assertTrue(any('SERIALIZABLE' in sql for sql in target.calls));self.assertTrue(any('READ ONLY' in sql for sql in source.calls));self.assertTrue(all(sql.startswith(('SELECT ','SET ')) for sql in source.calls))
  self.assertFalse(any(method in ('PUT','DELETE','PATCH') for url,method in calls if 'fixture.supabase.co/' in url))
 def test_interrupted_target_insert_and_readback_mismatch_roll_back_without_source_changes(self):
  for options in [dict(fail_table='activation'),dict(bad_readback=True)]:
   cfg,source,target,objects,destination,calls,connector,request,catalogs=self.fixture(**options);rows=copy.deepcopy(source.rows)
   with self.assertRaises((RuntimeError,migration.MigrationError)):migration.commit_migration(cfg,connector=connector,request=request,catalogs=catalogs)
   self.assertEqual(source.rows,rows);self.assertTrue(target.empty);self.assertTrue(all(not rows for rows in target.rows.values()));self.assertGreater(target.rollbacks,0);self.assertEqual(len(destination),3)
 def test_occupied_destination_conflict_never_overwrites_or_commits_database(self):
  cfg,source,target,objects,destination,calls,connector,request,catalogs=self.fixture();key=next(key for bucket,key in objects if bucket=='source-backups');destination[('wallet-backups',key)]=b'occupied evidence'
  with self.assertRaises(RemoteError):migration.commit_migration(cfg,connector=connector,request=request,catalogs=catalogs)
  self.assertEqual(destination[('wallet-backups',key)],b'occupied evidence');self.assertTrue(target.empty);self.assertFalse(any(sql.startswith('INSERT') for sql in target.calls))
 def test_missing_fence_relationship_or_source_digest_blocks_without_source_mutations(self):
  for case in ['fence','relationship','digest']:
   cfg,source,target,objects,destination,calls,connector,request,catalogs=self.fixture()
   if case=='fence':cfg.pop('MIGRATION_SOURCE_WRITES_PAUSED')
   elif case=='relationship':source.rows['sessions'][0]=tuple(['exact-session-digest','missing-owner',4102444800,'synthetic-device',1793000000])
   else:
    key=next(key for key in objects if key[0]=='source-backups');objects[key]=b'x'*len(objects[key])
   rows=copy.deepcopy(source.rows)
   with self.assertRaises(migration.MigrationError):migration.commit_migration(cfg,connector=connector,request=request,catalogs=catalogs)
   self.assertEqual(source.rows,rows);self.assertTrue(target.empty)
   if case=='fence':self.assertEqual(calls,[])
