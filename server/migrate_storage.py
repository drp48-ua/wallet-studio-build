"""Private migration: read-only preflight by default, explicitly fenced copy only.

Credentials are environment-only. The host must arrange and confirm write
fencing before commit. No source mutation, dumps, cutover or implicit activation.
"""
import argparse,hashlib,itertools,json,math,os,re,sys,time
from remote import PgDatabase,R2Blobs,SupabaseBlobs,TABLES,SCHEMA,MAX_REMOTE_BUNDLE,LOCK_ID
from designs import load_catalog,load_legacy_catalog,DEFAULT_CATALOG_PATH,DEFAULT_LEGACY_CATALOG_PATH,object_key,verify_asset,validate_png

MAX_OBJECTS=10000
MAX_TOTAL_BYTES=2*1024*1024*1024
DESIGN_LIMIT=4*1024*1024
MAX_DATABASE_ROWS=1000000
MAX_DATABASE_BYTES=64*1024*1024
MAX_MIGRATION_SECONDS=1800
FIELDS={
 'users':(('id','text'),('email','text'),('salt','bytea'),('password','bytea')),
 'sessions':(('digest','text'),('user_id','text'),('expires','bigint'),('device_id','text'),('last_seen','bigint')),
 'bundles':(('user_id','text'),('id','text'),('size','bigint')),
 'purchase_orders':(('id','text'),('email','text'),('active','bigint'),('updated','double precision')),
 'purchase_packs':(('id','text'),('email','text'),('active','bigint'),('updated','double precision')),
 'webhook_events':(('id','text'),('received','bigint')),
 'activation':(('email','text'),('digest','text'),('expires','bigint'),('attempts','bigint'),('last_sent','bigint')),
 'computers':(('user_id','text'),('id','text'),('created','bigint'),('last_seen','bigint'),('peer_digest','text')),
}
PRIMARY={'users':('id',),'sessions':('digest',),'bundles':('user_id','id'),'purchase_orders':('id',),'purchase_packs':('id',),'webhook_events':('id',),'activation':('email',),'computers':('user_id','id')}

class MigrationError(ValueError):pass

def _deadline(end):
 if end is not None and time.monotonic()>=end:raise MigrationError('migration_time_limit')

def existing_private_supabase_bucket(blobs):
 status,body=blobs.request(blobs.base+'/bucket/'+blobs.bucket,'GET',blobs.headers,maximum=65536)
 if status!=200:raise MigrationError('source_bucket_unavailable')
 try:value=json.loads(body)
 except (ValueError,TypeError):raise MigrationError('source_bucket_invalid') from None
 if not isinstance(value,dict) or value.get('public') is not False:raise MigrationError('source_bucket_not_private')

def allowed_object_key(key,designs):
 if not isinstance(key,str):return False
 if designs:
  match=re.fullmatch(r'studio-50-v1/(?:native/)?ws-([0-9]{3})(?:-([0-9a-f]{64}))?\.png',key)
  return bool(match and 1<=int(match[1])<=100 and (('/native/' in key)==(match[2] is not None)))
 return re.fullmatch(r'[a-zA-Z0-9-]{1,64}/[0-9a-f]{64}',key) is not None

def inventory_supabase(blobs,*,designs=False,deadline=None):
 """List every object, including orphans/old variants. Unknown keys stop cutover."""
 existing_private_supabase_bucket(blobs)
 result={};folders=[''];seen=set();total=0;listed=0
 while folders:
  prefix=folders.pop()
  if prefix in seen:raise MigrationError('source_listing_cycle')
  seen.add(prefix);offset=0
  while True:
   _deadline(deadline)
   payload=json.dumps({'prefix':prefix,'limit':100,'offset':offset,'sortBy':{'column':'name','order':'asc'}}).encode()
   status,body=blobs.request(blobs.base+'/object/list/'+blobs.bucket,'POST',dict(blobs.headers,**{'Content-Type':'application/json'}),payload,1024*1024)
   if status!=200:raise MigrationError('source_listing_unavailable')
   try:page=json.loads(body)
   except (ValueError,TypeError):raise MigrationError('source_listing_invalid') from None
   if not isinstance(page,list) or len(page)>100:raise MigrationError('source_listing_invalid')
   for entry in page:
    listed+=1
    if listed>MAX_OBJECTS+1000:raise MigrationError('source_listing_limit')
    if not isinstance(entry,dict) or not isinstance(entry.get('name'),str) or not re.fullmatch('[a-zA-Z0-9][a-zA-Z0-9_.-]{0,255}',entry['name']):raise MigrationError('source_listing_invalid')
    key=prefix+entry['name']
    if entry.get('id') is None and entry.get('metadata') is None:
     if key.count('/')>=2:raise MigrationError('source_listing_depth')
     folders.append(key+'/');continue
    metadata=entry.get('metadata');size=metadata.get('size') if isinstance(metadata,dict) else None
    if not allowed_object_key(key,designs) or type(size) is not int or not 0<size<=(DESIGN_LIMIT if designs else MAX_REMOTE_BUNDLE) or key in result:raise MigrationError('source_object_requires_review')
    result[key]=size;total+=size
    if len(result)>MAX_OBJECTS or total>MAX_TOTAL_BYTES:raise MigrationError('source_inventory_limit')
   if len(page)<100:break
   offset+=100
 return result

def inspect_database(connection,*,target=False):
 """Schema-only/aggregate checks; never emit credentials, users or object keys."""
 tables=connection.execute('SELECT tablename FROM pg_tables WHERE schemaname=%s',(SCHEMA,)).fetchall()
 names={row[0] for row in tables}
 if target and not names:return {'tables':0,'rows':0,'empty':True}
 if names!=set(TABLES):raise MigrationError('database_schema_requires_review')
 columns=connection.execute('SELECT table_name,column_name,data_type,is_nullable FROM information_schema.columns WHERE table_schema=%s ORDER BY table_name,ordinal_position',(SCHEMA,)).fetchall()
 expected=sorted((table,index,name,kind) for table,fields in FIELDS.items() for index,(name,kind) in enumerate(fields))
 observed=[];positions={}
 for table,name,kind,nullable in columns:
  if nullable!='NO':raise MigrationError('database_columns_require_review')
  index=positions.get(table,0);positions[table]=index+1;observed.append((table,index,name,kind))
 if sorted(observed)!=expected:raise MigrationError('database_columns_require_review')
 constraints=connection.execute("""SELECT t.relname,c.contype,ARRAY(SELECT a.attname FROM unnest(c.conkey) WITH ORDINALITY u(num,ord) JOIN pg_attribute a ON a.attrelid=t.oid AND a.attnum=u.num ORDER BY u.ord) FROM pg_constraint c JOIN pg_class t ON t.oid=c.conrelid JOIN pg_namespace n ON n.oid=t.relnamespace WHERE n.nspname=%s AND c.contype<>'n'""",(SCHEMA,)).fetchall()
 actual={(table,kind,tuple(fields)) for table,kind,fields in constraints}
 wanted={(table,'p',fields) for table,fields in PRIMARY.items()}|{('users','u',('email',))}
 if actual!=wanted:raise MigrationError('database_constraints_require_review')
 indexes=connection.execute("""SELECT t.relname,i.relname,ARRAY(SELECT a.attname FROM unnest(x.indkey) WITH ORDINALITY u(num,ord) JOIN pg_attribute a ON a.attrelid=t.oid AND a.attnum=u.num ORDER BY u.ord),x.indexprs IS NULL AND x.indpred IS NULL FROM pg_index x JOIN pg_class t ON t.oid=x.indrelid JOIN pg_class i ON i.oid=x.indexrelid JOIN pg_namespace n ON n.oid=t.relnamespace WHERE n.nspname=%s AND NOT EXISTS(SELECT 1 FROM pg_constraint c WHERE c.conindid=i.oid)""",(SCHEMA,)).fetchall()
 if {(table,name,tuple(fields),plain) for table,name,fields,plain in indexes}!={('purchase_orders','purchase_email',('email','active'),True),('purchase_packs','purchase_pack_email',('email','active'),True)}:raise MigrationError('database_indexes_require_review')
 private=connection.execute("""SELECT NOT EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=%s AND c.relkind='r' AND (NOT c.relrowsecurity OR EXISTS(SELECT 1 FROM aclexplode(coalesce(c.relacl,acldefault('r',c.relowner))) a LEFT JOIN pg_roles r ON r.oid=a.grantee WHERE a.grantee=0 OR r.rolname IN ('anon','authenticated')))) AND NOT EXISTS(SELECT 1 FROM pg_namespace n CROSS JOIN LATERAL aclexplode(coalesce(n.nspacl,acldefault('n',n.nspowner))) a LEFT JOIN pg_roles r ON r.oid=a.grantee WHERE n.nspname=%s AND (a.grantee=0 OR r.rolname IN ('anon','authenticated')))""",(SCHEMA,SCHEMA)).fetchone()[0]
 if private is not True:raise MigrationError('database_privacy_requires_review')
 counts={table:connection.execute('SELECT count(*) FROM '+SCHEMA+'.'+table).fetchone()[0] for table in TABLES}
 if any(type(count) is not int or count<0 for count in counts.values()):raise MigrationError('database_counts_invalid')
 rows=sum(counts.values())
 if target and rows:raise MigrationError('target_database_not_empty')
 return {'tables':len(names),'rows':rows,'empty':rows==0}

def preflight(config,*,connector=None,request=None,deadline=None):
 deadline=time.monotonic()+MAX_MIGRATION_SECONDS if deadline is None else deadline
 _deadline(deadline)
 required=('MIGRATION_SOURCE_DATABASE_URL','MIGRATION_TARGET_DATABASE_URL','MIGRATION_SOURCE_SUPABASE_URL','MIGRATION_SOURCE_SUPABASE_KEY','MIGRATION_SOURCE_BACKUP_BUCKET','R2_ACCOUNT_ID','R2_ACCESS_KEY_ID','R2_SECRET_ACCESS_KEY','CLOUDFLARE_API_TOKEN','R2_BACKUP_BUCKET','R2_DESIGN_BUCKET')
 if any(not config.get(key) for key in required) or config['MIGRATION_SOURCE_DATABASE_URL']==config['MIGRATION_TARGET_DATABASE_URL'] or config['R2_BACKUP_BUCKET']==config['R2_DESIGN_BUCKET']:raise MigrationError('migration_configuration_invalid')
 kwargs={} if request is None else {'request':request}
 inventories=[]
 for designs in (False,True):
  source=SupabaseBlobs(config['MIGRATION_SOURCE_SUPABASE_URL'],config['MIGRATION_SOURCE_SUPABASE_KEY'],'wallet-designs' if designs else config['MIGRATION_SOURCE_BACKUP_BUCKET'],**kwargs)
  target=R2Blobs(config,config['R2_DESIGN_BUCKET' if designs else 'R2_BACKUP_BUCKET'],**kwargs);target.verify_bucket()
  objects=inventory_supabase(source,designs=designs,deadline=deadline);inventories.append({'objects':len(objects),'bytes':sum(objects.values())})
 results=[]
 for target in (False,True):
  db=PgDatabase(config['MIGRATION_TARGET_DATABASE_URL' if target else 'MIGRATION_SOURCE_DATABASE_URL'],connector=connector)
  connection=db.open()
  try:
   with connection.transaction():
    connection.execute('SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY')
    connection.execute('SET LOCAL statement_timeout=60000');_deadline(deadline)
    results.append(inspect_database(connection,target=target))
  finally:connection.close()
 return {'result':'preflight_only','database_checks':results,'backup_inventory':inventories[0],'design_inventory':inventories[1],'source_unchanged':True,'commit_available':True}

def copy_objects(source,target,inventory,*,designs=False,expected=None,deadline=None):
 """Create-only copy with full authenticated source/destination verification.

 Existing equal objects are resumable; conflicting bytes never get replaced.
 Orphans and superseded native assets remain objects, never new DB authority.
 """
 target.verify_bucket();existing_private_supabase_bucket(source)
 expected={} if expected is None else expected
 if not set(expected).issubset(inventory):raise MigrationError('source_catalog_object_missing')
 count=0;total=0
 for key,size in sorted(inventory.items()):
  _deadline(deadline)
  if not allowed_object_key(key,designs) or type(size) is not int or not 0<size<=(DESIGN_LIMIT if designs else MAX_REMOTE_BUNDLE):raise MigrationError('source_object_requires_review')
  status,data=source.request(source.base+'/object/authenticated/'+source.bucket+'/'+key,'GET',source.headers,maximum=DESIGN_LIMIT if designs else MAX_REMOTE_BUNDLE)
  if status!=200 or len(data)!=size:raise MigrationError('source_object_unavailable')
  digest=hashlib.sha256(data).hexdigest()
  if designs:
   validate_png(data)
   if '/native/' in key and key.rsplit('-',1)[1][:-4]!=digest:raise MigrationError('source_object_digest_mismatch')
   if key in expected:verify_asset(expected[key],data)
  elif key.rsplit('/',1)[1]!=digest:raise MigrationError('source_object_digest_mismatch')
  target.create_key(key,data,DESIGN_LIMIT if designs else MAX_REMOTE_BUNDLE,'image/png' if designs else 'application/zip')
  _deadline(deadline)
  count+=1;total+=size
  if count>MAX_OBJECTS or total>MAX_TOTAL_BYTES:raise MigrationError('source_inventory_limit')
 return {'objects':count,'bytes':total}

def _rows(connection,table):
 columns=','.join(name for name,_ in FIELDS[table]);order=','.join(name+' COLLATE "C"' for name in PRIMARY[table])
 with connection.cursor(name='wallet_migration_'+table) as cursor:
  cursor.execute('SELECT '+columns+' FROM '+SCHEMA+'.'+table+' ORDER BY '+order)
  while True:
   batch=cursor.fetchmany(32)
   if not batch:break
   for row in batch:
    if len(row)!=len(FIELDS[table]):raise MigrationError('database_row_requires_review')
    normal=[];size=0
    for value,(_,kind) in zip(row,FIELDS[table]):
     if isinstance(value,memoryview):value=value.tobytes()
     if ((kind=='text' and not isinstance(value,str)) or (kind=='bytea' and not isinstance(value,bytes)) or (kind=='bigint' and type(value) is not int)
         or (kind=='double precision' and (type(value) is not float or not math.isfinite(value)))):raise MigrationError('database_row_requires_review')
     size+=len(value.encode('utf-8') if isinstance(value,str) else value) if isinstance(value,(str,bytes)) else 8
     normal.append(value)
    if size>1024*1024:raise MigrationError('database_row_requires_review')
    yield tuple(normal),size

def _check_relations(connection):
 # Expired historical empty-device sessions are retained byte-for-byte; only
 # nonempty device references must resolve to the same owner's computer row.
 checks=("SELECT count(*) FROM wallet_private.sessions x LEFT JOIN wallet_private.users u ON u.id=x.user_id WHERE u.id IS NULL",
         "SELECT count(*) FROM wallet_private.computers x LEFT JOIN wallet_private.users u ON u.id=x.user_id WHERE u.id IS NULL",
         "SELECT count(*) FROM wallet_private.bundles x LEFT JOIN wallet_private.users u ON u.id=x.user_id WHERE u.id IS NULL",
         "SELECT count(*) FROM wallet_private.sessions x LEFT JOIN wallet_private.computers c ON c.user_id=x.user_id AND c.id=x.device_id WHERE x.device_id<>'' AND c.id IS NULL")
 if any(connection.execute(sql).fetchone()[0]!=0 for sql in checks):raise MigrationError('database_relationship_requires_review')

def _check_schema_extras(connection):
 if connection.execute('SELECT count(*) FROM pg_policies WHERE schemaname=%s',(SCHEMA,)).fetchone()[0]:raise MigrationError('database_policy_requires_review')
 if connection.execute("SELECT count(*) FROM pg_trigger g JOIN pg_class c ON c.oid=g.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=%s AND NOT g.tgisinternal",(SCHEMA,)).fetchone()[0]:raise MigrationError('database_trigger_requires_review')
 defaults=connection.execute('SELECT table_name,column_name,column_default FROM information_schema.columns WHERE table_schema=%s ORDER BY table_name,ordinal_position',(SCHEMA,)).fetchall()
 wanted={(table,name,"''::text" if (table,name)==('sessions','device_id') else '0' if (table,name)==('sessions','last_seen') else None) for table,fields in FIELDS.items() for name,_ in fields}
 if set(defaults)!=wanted:raise MigrationError('database_defaults_require_review')

def _create_private_schema(connection):
 connection.execute('CREATE SCHEMA IF NOT EXISTS wallet_private');connection.execute('REVOKE ALL ON SCHEMA wallet_private FROM PUBLIC')
 roles=[role for role in ('anon','authenticated') if connection.execute('SELECT 1 FROM pg_roles WHERE rolname=%s',(role,)).fetchone()]
 for role in roles:connection.execute('REVOKE ALL ON SCHEMA wallet_private FROM '+role)
 for table,fields in FIELDS.items():
  definitions=[]
  for name,kind in fields:
   default=" DEFAULT ''" if (table,name)==('sessions','device_id') else ' DEFAULT 0' if (table,name)==('sessions','last_seen') else ''
   definitions.append(name+' '+kind.upper()+' NOT NULL'+default)
  definitions.append('PRIMARY KEY('+','.join(PRIMARY[table])+')')
  if table=='users':definitions.append('UNIQUE(email)')
  connection.execute('CREATE TABLE IF NOT EXISTS wallet_private.'+table+'('+','.join(definitions)+')')
  connection.execute('ALTER TABLE wallet_private.'+table+' ENABLE ROW LEVEL SECURITY')
  for role in ['PUBLIC']+roles:connection.execute('REVOKE ALL ON TABLE wallet_private.'+table+' FROM '+role)
 for table,name in [('purchase_orders','purchase_email'),('purchase_packs','purchase_pack_email')]:connection.execute('CREATE INDEX IF NOT EXISTS '+name+' ON wallet_private.'+table+'(email,active)')

def copy_database(source,target,*,expected_rows,deadline=None):
 """Caller owns one SERIALIZABLE target transaction; any mismatch must escape it."""
 inspect_database(target,target=True);_create_private_schema(target);_check_schema_extras(target)
 count=0;total=0
 for table in TABLES:
  columns=','.join(name for name,_ in FIELDS[table]);placeholders=','.join('%s' for _ in FIELDS[table])
  for row,size in _rows(source,table):
   _deadline(deadline)
   count+=1;total+=size
   if count>MAX_DATABASE_ROWS or total>MAX_DATABASE_BYTES:raise MigrationError('database_copy_limit')
   target.execute('INSERT INTO wallet_private.'+table+'('+columns+') VALUES('+placeholders+')',row)
 if count!=expected_rows:raise MigrationError('database_row_count_mismatch')
 _check_relations(target);inspect_database(target)
 sentinel=object()
 for table in TABLES:
  for left,right in itertools.zip_longest(_rows(source,table),_rows(target,table),fillvalue=sentinel):
   if left is sentinel or right is sentinel or left!=right:raise MigrationError('database_readback_mismatch')
 _deadline(deadline)
 return {'tables':len(TABLES),'rows':count,'exact_readback':True}

def commit_migration(config,*,connector=None,request=None,catalogs=None):
 """Explicit, host-fenced migration. No automatic endpoint/env activation.

 The operator must stop and drain source writers/webhooks BEFORE confirming.
 Advisory locks additionally serialize cooperating backend transactions. R2
 objects already copied survive a DB rollback safely for a later explicit run.
 """
 if config.get('MIGRATION_SOURCE_WRITES_PAUSED')!='confirmed':raise MigrationError('source_writes_not_fenced')
 deadline=time.monotonic()+MAX_MIGRATION_SECONDS
 preflight(config,connector=connector,request=request,deadline=deadline)
 kwargs={} if request is None else {'request':request}
 catalogs=[load_catalog(DEFAULT_CATALOG_PATH),load_legacy_catalog(DEFAULT_LEGACY_CATALOG_PATH)] if catalogs is None else catalogs
 expected={object_key(item):item for catalog in catalogs for item in catalog['designs']}
 source=PgDatabase(config['MIGRATION_SOURCE_DATABASE_URL'],connector=connector).open()
 target=None
 try:
  with source.transaction():
   source.execute('SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY');source.execute('SET LOCAL statement_timeout=60000');source.execute('SELECT pg_advisory_xact_lock(%s)',(LOCK_ID,));_deadline(deadline)
   summary=inspect_database(source);_check_schema_extras(source);_check_relations(source)
   if summary['rows']>MAX_DATABASE_ROWS:raise MigrationError('database_copy_limit')
   copies=[];inventories=[];sources=[];targets=[]
   for designs in (False,True):
    blobs=SupabaseBlobs(config['MIGRATION_SOURCE_SUPABASE_URL'],config['MIGRATION_SOURCE_SUPABASE_KEY'],'wallet-designs' if designs else config['MIGRATION_SOURCE_BACKUP_BUCKET'],**kwargs)
    destination=R2Blobs(config,config['R2_DESIGN_BUCKET' if designs else 'R2_BACKUP_BUCKET'],**kwargs)
    inventory=inventory_supabase(blobs,designs=designs,deadline=deadline);sources.append(blobs);targets.append(destination);inventories.append(inventory)
   for row,_ in _rows(source,'bundles'):
    user,ident,size=row
    if inventories[0].get(user+'/'+ident)!=size:raise MigrationError('indexed_bundle_missing_or_changed')
   if not set(expected).issubset(inventories[1]):raise MigrationError('source_catalog_object_missing')
   for index in range(2):copies.append(copy_objects(sources[index],targets[index],inventories[index],designs=index==1,expected=expected if index==1 else None,deadline=deadline))
   # Recheck listings and privacy under the fence before granting DB authority.
   for index in range(2):
    targets[index].verify_bucket()
    if inventory_supabase(sources[index],designs=index==1,deadline=deadline)!=inventories[index]:raise MigrationError('source_inventory_changed')
   target=PgDatabase(config['MIGRATION_TARGET_DATABASE_URL'],connector=connector).open()
   with target.transaction():
    target.execute('SET TRANSACTION ISOLATION LEVEL SERIALIZABLE');target.execute('SET LOCAL statement_timeout=60000');target.execute('SELECT pg_advisory_xact_lock(%s)',(LOCK_ID,))
    result=copy_database(source,target,expected_rows=summary['rows'],deadline=deadline)
  return {'result':'copied_not_activated','database':result,'backup_objects':copies[0],'design_objects':copies[1],'source_unchanged':True}
 finally:
  if target is not None:target.close()
  source.close()

def main(argv=None,config=None):
 parser=argparse.ArgumentParser(description='Read-only private migration preflight; credentials from environment only.')
 parser.add_argument('--commit',action='store_true',help='Explicit immutable copy; also requires MIGRATION_SOURCE_WRITES_PAUSED=confirmed after host-maintained write fencing.')
 args=parser.parse_args(argv)
 try:
  settings=os.environ if config is None else config
  print(json.dumps(commit_migration(settings) if args.commit else preflight(settings)));return 0
 except MigrationError as error:print(json.dumps({'result':'blocked','category':str(error)}));return 1
 except Exception:print(json.dumps({'result':'blocked','category':'migration_preflight_failed'}));return 1

if __name__=='__main__':sys.exit(main())
