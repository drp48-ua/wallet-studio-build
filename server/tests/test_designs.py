import base64,hashlib,hmac,json,pathlib,struct,sys,tempfile,threading,unittest,urllib.request,urllib.error,zlib
from unittest.mock import patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import test_accounts
from app import Server
from accounts import Accounts,AccountError
from designs import load_catalog,verify_asset,validate_png,DesignCatalog
from upload_designs import provision

def png():
 def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
 return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',320,200,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+b'\x22\x33\x44'*320)*200))+chunk(b'IEND',b'')

def fixture_catalog(data):
 return dict(schema=1,pack_id='studio-50-v1',title='50 diseños',designs=[dict(id=f'ws-{i:03}',filename=f'ws-{i:03}.png',title=f'Diseño {i}',sha256=hashlib.sha256(data).hexdigest(),bytes=len(data),width=320,height=200) for i in range(1,51)])

class PaidDesignTests(unittest.TestCase):
 setUp=test_accounts.AccountTests.setUp
 tearDown=test_accounts.AccountTests.tearDown
 order=test_accounts.AccountTests.order
 webhook=test_accounts.AccountTests.webhook
 def configure(self):
  self.config.update(SHOPIFY_APP_PRODUCT_IDS='123',SHOPIFY_BUNDLE_PRODUCT_IDS='456',SHOPIFY_PACK_PRODUCT_IDS='789')
  self.auth=Accounts(self.store,self.config,lambda email,code:self.mail.append((email,code)));self.store.accounts=self.auth
 def activate_order(self,product=123,email='buyer@example.test',ident=1):
  self.webhook(self.order(product=product,email=email,ident=ident),str(ident));self.auth.start(email)
  return self.auth.activate(email,self.mail[-1][1],'strong-fixture-password','strong-fixture-password','1'*64,'x')
 def test_single_product_and_legacy_app_both_include_designs_while_historical_pack_never_grants_app(self):
  self.configure()
  for i,(product,app,pack) in enumerate([(123,True,True),(789,False,True),(456,True,True)],1):
   session=self.activate_order(product,f'buyer{i}@example.test',i)
   self.assertEqual(self.auth.app_entitled(session['user_id']),app);self.assertEqual(self.auth.pack_entitled(session['user_id']),pack)
   self.assertEqual(self.auth.subject(session['access_token']) is not None,app)
   self.assertEqual(self.auth.subject(session['access_token'],False),session['user_id'])
 def test_historical_pack_refund_cannot_remove_designs_included_with_active_app_and_device_revocation(self):
  self.configure();session=self.activate_order()
  body=self.order(ident=2,product=789);self.webhook(body,'pack');self.assertTrue(self.auth.pack_entitled(session['user_id']))
  body.update(updated_at='2026-10-15T12:01:00Z',financial_status='partially_refunded',refunds=[{'refund_line_items':[{'line_item_id':99,'quantity':1}]}])
  self.webhook(body,'refund','orders/updated');self.webhook(body,'refund','orders/updated');self.webhook(self.order(ident=2,product=789),'stale')
  self.assertTrue(self.auth.pack_entitled(session['user_id']));self.assertEqual(self.auth.subject(session['access_token']),session['user_id'])
  self.auth.revoke(session['user_id'],'1'*64);self.assertIsNone(self.auth.subject(session['access_token'],False))
 def test_refund_notification_fail_closes_and_new_order_reconciles(self):
  self.configure();session=self.activate_order(456)
  self.webhook({'order_id':1,'created_at':'2026-10-15T12:02:00Z'},'refund','refunds/create')
  self.assertIsNone(self.auth.subject(session['access_token'],False));self.assertFalse(self.auth.pack_entitled(session['user_id']))
  self.webhook(self.order(product=456),'stale');self.assertFalse(self.auth.pack_entitled(session['user_id']))
  self.webhook(self.order(product=456,updated='2026-10-15T12:03:00Z'),'reconcile','orders/updated');self.assertTrue(self.auth.pack_entitled(session['user_id']))
 def test_refund_before_paid_keeps_tombstone_and_cancellation_revokes_bundle(self):
  self.configure()
  self.webhook({'order_id':1,'created_at':'2026-10-15T12:02:00Z'},'early-refund','refunds/create')
  self.webhook(self.order(product=456),'stale-paid')
  with self.store.connect() as db:
   self.assertFalse(self.auth.entitled(db,'buyer@example.test'))
  self.webhook(self.order(product=456,updated='2026-10-15T12:03:00Z'),'new-paid')
  self.auth.start('buyer@example.test');session=self.auth.activate('buyer@example.test',self.mail[-1][1],'strong-fixture-password','strong-fixture-password','1'*64,'x')
  self.webhook(self.order(product=456,updated='2026-10-15T12:04:00Z'),'cancel','orders/cancelled')
  self.assertIsNone(self.auth.subject(session['access_token'],False));self.assertFalse(self.auth.pack_entitled(session['user_id']))
 def test_current_single_sku_and_legacy_app_upgrade_work_with_no_pack_product_configured(self):
  self.configure();self.config['SHOPIFY_PACK_PRODUCT_IDS']='';self.config['SHOPIFY_BUNDLE_PRODUCT_IDS']='';self.auth=Accounts(self.store,self.config,lambda email,code:self.mail.append((email,code)));self.store.accounts=self.auth
  for index,product in enumerate([123],1):
   session=self.activate_order(product,f'owner{index}@example.test',index)
   self.assertTrue(self.auth.app_entitled(session['user_id']));self.assertTrue(self.auth.pack_entitled(session['user_id']))
   self.webhook(self.order(ident=index,email=f'owner{index}@example.test',product=product,status='refunded',updated='2026-10-15T12:05:00Z'),'refund'+str(index),'orders/updated')
   self.assertFalse(self.auth.app_entitled(session['user_id']));self.assertFalse(self.auth.pack_entitled(session['user_id']))
  self.webhook(self.order(ident=9,product=789),'archived-pack');self.auth.start('buyer@example.test');self.assertNotIn('buyer@example.test',[email for email,_ in self.mail])
 def test_existing_orders_and_sessions_survive_new_schema(self):
  session=self.activate_order();self.configure();self.assertEqual(self.auth.subject(session['access_token']),session['user_id']);self.assertTrue(self.auth.pack_entitled(session['user_id']))
 def test_authenticated_http_account_isolation_and_pack_only_no_bundle_access(self):
  self.configure();app=self.activate_order();pack=self.activate_order(789,'pack@example.test',2);bundle=self.activate_order(456,'bundle@example.test',3)
  data=png();catalog=fixture_catalog(data);self.store.design_catalog.catalog=lambda capability=None:catalog;self.store.design_catalog.read=lambda item,capability=None:data
  server=Server(('127.0.0.1',0),self.store);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
  def request(path,token=None):
   headers={} if token is None else {'Authorization':'Bearer '+token}
   try:
    with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.server_port}'+path,headers=headers)) as response:return response.status,response.read()
   except urllib.error.HTTPError as error:return error.code,error.read()
  try:
   self.assertEqual(request('/v1/designs')[0],401)
   self.assertTrue(json.loads(request('/v1/designs',app['access_token'])[1])['entitled'])
   self.assertEqual(request('/v1/designs/ws-001',app['access_token']),(200,data))
   self.assertEqual(request('/v1/designs/ws-001?user_id='+bundle['user_id'],'0'*64)[0],401)
   self.assertEqual(request('/v1/designs/ws-001',pack['access_token']),(200,data))
   self.assertEqual(request('/v1/bundles',pack['access_token'])[0],401)
   self.assertEqual(request('/v1/auth/session',pack['access_token'])[0],401)
   self.assertFalse(json.loads(request('/v1/auth/account',pack['access_token'])[1])['app_licensed'])
   self.webhook(self.order(ident=2,product=789,updated='2026-10-15T12:03:00Z',status='refunded'),'revoke','orders/updated')
   self.assertEqual(request('/v1/designs/ws-001',pack['access_token'])[0],401)
   self.assertEqual(request('/v1/designs/ws-001',bundle['access_token']),(200,data))
   self.store.design_catalog.read=lambda item,capability=None:(_ for _ in ()).throw(ValueError('corrupt fixture'))
   self.assertEqual(request('/v1/designs/ws-001',bundle['access_token'])[0],503)
  finally:server.shutdown();server.server_close();thread.join()
 def test_catalog_png_and_provisioning_are_bounded_private_and_verified(self):
  data=png();catalog=fixture_catalog(data);root=pathlib.Path(self.tmp.name);catalog_path=root/'catalog.json';catalog_path.write_text(json.dumps(catalog));self.assertEqual(len(load_catalog(catalog_path)['designs']),50)
  for item in catalog['designs']:(root/item['filename']).write_bytes(data)
  objects={};calls=[];bucket=None
  def remote(url,method,headers,body=None,maximum=0):
   nonlocal bucket
   calls.append((url,method,headers))
   if url.endswith('/bucket/wallet-designs'):return (404,b'{}') if bucket is None else (200,json.dumps(bucket).encode())
   if url.endswith('/bucket'):
    bucket=json.loads(body);return 201,b'{}'
   key=url.rsplit('/',1)[-1]
   if method=='POST':
    if key in objects:return 409,b'{}'
    objects[key]=body;return 201,b'{}'
   return 200,objects[key]
  self.assertEqual(provision({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'},root,catalog_path,remote),50)
  self.assertFalse(bucket['public']);self.assertEqual(bucket['allowed_mime_types'],['image/png']);self.assertTrue(all('/public/' not in call[0] for call in calls))
  self.assertEqual(provision({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'},root,catalog_path,remote),50)
  objects['ws-001.png']=b'occupied-corrupt-object'
  with self.assertRaises(ValueError):provision({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'},root,catalog_path,remote)
  self.assertEqual(objects['ws-001.png'],b'occupied-corrupt-object')
  bucket['public']=True
  with self.assertRaises(ValueError):provision({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'},root,catalog_path,remote)

  for malformed in [b'bad',data[:-1],data+b'junk',data[:25]+b'bad'+data[28:]]:
   with self.assertRaises(ValueError):validate_png(malformed)
  with self.assertRaises(ValueError):verify_asset(catalog['designs'][0],data+b'x')
  catalog['designs'][0]['filename']='../escape.png';catalog_path.write_text(json.dumps(catalog))
  with self.assertRaises(ValueError):load_catalog(catalog_path)

 def test_storage_reader_never_accepts_public_bucket_or_corrupt_png(self):
  data=png();item=fixture_catalog(data)['designs'][0]
  catalog=DesignCatalog({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'})
  class Blobs:
   base='https://fixture.supabase.co/storage/v1';headers={'Authorization':'Bearer fixture-service'}
   def __init__(self,public,payload):self.public=public;self.payload=payload;self.reads=0
   def request(self,url,method,headers,body=None,maximum=0):
    if '/bucket/' in url:return 200,json.dumps({'public':self.public}).encode()
    self.reads+=1
    if '/object/authenticated/wallet-designs/studio-50-v1/' not in url:raise AssertionError('not authenticated')
    return 200,self.payload
  for public,payload in [(True,data),(False,b'corrupt')]:
   blobs=Blobs(public,payload);catalog.blobs=lambda:blobs
   with self.assertRaises(ValueError):catalog.read(item)
   if public:self.assertEqual(blobs.reads,0)
  blobs=Blobs(False,data);catalog.blobs=lambda:blobs;self.assertEqual(catalog.read(item),data)
