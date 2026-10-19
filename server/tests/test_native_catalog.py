"""Native 2K (1536x969) catalogue assets up to 4 MiB with full 320x200 legacy compatibility."""
import hashlib,json,pathlib,struct,sys,tempfile,threading,unittest,urllib.request,zlib
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from app import Server
from designs import (ASSET_LIMITS,DesignCatalog,EXPANDED_CATALOG_CAPABILITY,EXPANDED_CATALOG_HEADER,LEGACY_ASSET,LEGACY_SIZE,
                     MAX_ASSET,NATIVE_2K_CAPABILITY,NATIVE_VARIANT_CAPABILITY,NATIVE_SIZE,catalog_for_client,load_catalog,validate_png)
import test_designs as fixtures
from test_designs import fixture_catalog,png

def png_at(width,height,fill=b'\x22\x33\x44'):
 def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
 return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',width,height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+fill*width)*height))+chunk(b'IEND',b'')

def native_metadata(data,count=6):
 template=dict(id='ws-051',filename='ws-051.png',title='Nativo',sha256=hashlib.sha256(data).hexdigest(),bytes=max(len(data),1),width=1536,height=969)
 return [dict(template,id=f'ws-{index:03}',filename=f'ws-{index:03}.png') for index in range(51,51+count)]

class NativeAssetValidationTests(unittest.TestCase):
 def setUp(self):self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
 def load(self,value):
  path=pathlib.Path(self.tmp.name)/'catalog.json';path.write_text(json.dumps(value),encoding='utf-8')
  return load_catalog(path)
 def test_contract_constants_match_the_shipped_windows_client(self):
  # The Windows client sends this exact capability value; keep in sync with CATALOG_CAPABILITY in design_library.rs.
  self.assertEqual((LEGACY_ASSET,MAX_ASSET,LEGACY_SIZE,NATIVE_SIZE),(1024*1024,4*1024*1024,(320,200),(1536,969)))
  self.assertEqual(NATIVE_2K_CAPABILITY,'native-2k-v1')
  self.assertEqual(ASSET_LIMITS,{LEGACY_SIZE:LEGACY_ASSET,NATIVE_SIZE:MAX_ASSET})
 def test_validate_png_accepts_the_native_canvas_and_rejects_unknown_or_oversized_assets(self):
  validate_png(png_at(1536,969))
  for width,height in [(1536,968),(1537,969),(320,201),(321,200),(640,480)]:
   with self.subTest(geometry=(width,height)),self.assertRaises(ValueError):validate_png(png_at(width,height))
  with self.assertRaises(ValueError):validate_png(png_at(320,200)+b'\0'*LEGACY_ASSET)
  with self.assertRaises(ValueError):validate_png(b'\x89PNG\r\n\x1a\n'+b'\0'*MAX_ASSET)
 def test_load_catalog_accepts_native_and_legacy_catalogs_and_rejects_oversized_or_corrupt(self):
  data=png();base=fixture_catalog(data)
  self.assertEqual(len(self.load(base)['designs']),50)
  extended=fixture_catalog(data);extended['designs']+=native_metadata(data)
  self.assertEqual(len(self.load(extended)['designs']),56)
  all_native=dict(schema=1,pack_id='studio-50-v1',title='56 diseños',designs=[dict(id=f'ws-{i:03}',filename=f'ws-{i:03}.png',title=f'Diseño {i}',sha256='0'*64,bytes=1024,width=1536,height=969) for i in range(1,57)])
  self.assertEqual(len(self.load(all_native)['designs']),56)
  heavy=json.loads(json.dumps(extended));heavy['designs'][50]['bytes']=MAX_ASSET+1
  with self.assertRaises(ValueError):self.load(heavy)
  bad_dim=json.loads(json.dumps(all_native));bad_dim['designs'][0]['width']=1537
  with self.assertRaises(ValueError):self.load(bad_dim)
  fat=json.loads(json.dumps(base));fat['designs'][0]['bytes']=LEGACY_ASSET+1
  with self.assertRaises(ValueError):self.load(fat)
 def test_catalog_for_client_serves_native_assets_only_for_the_native_capability(self):
  data=png();catalog=fixture_catalog(data);catalog['designs']+=native_metadata(data,count=3)
  legacy=catalog_for_client(catalog,None)
  self.assertEqual(legacy['designs'],catalog['designs'][:50]);self.assertEqual(legacy['title'],'50 diseños')
  expanded=catalog_for_client(catalog,EXPANDED_CATALOG_CAPABILITY)
  self.assertEqual(expanded['designs'],catalog['designs'][:50]);self.assertEqual(expanded['title'],'50 diseños')
  self.assertEqual(catalog_for_client(catalog,NATIVE_2K_CAPABILITY)['designs'],catalog['designs'][:50])
  native=catalog_for_client(catalog,NATIVE_VARIANT_CAPABILITY)
  self.assertEqual(native['designs'],catalog['designs']);self.assertEqual(native['title'],'53 diseños');self.assertTrue(native['entitled'])
  self.assertEqual(catalog_for_client(catalog,'unknown-future-value')['designs'],legacy['designs'])
 def test_storage_reader_verifies_native_assets_against_metadata_with_a_four_mib_read_cap(self):
  data=png_at(1536,969)
  item=dict(id='ws-051',filename='ws-051.png',title='Nativo',sha256=hashlib.sha256(data).hexdigest(),bytes=len(data),width=1536,height=969)
  catalog=DesignCatalog({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'})
  reads=[]
  class Blobs:
   base='https://fixture.supabase.co/storage/v1';headers={'Authorization':'Bearer fixture-service'}
   def request(self,url,method,headers,body=None,maximum=0):
    if '/bucket/' in url:return 200,b'{"public":false}'
    reads.append((url,maximum));return 200,data
  blobs=Blobs();catalog.blobs=lambda:blobs
  self.assertEqual(catalog.read(item),data)
  self.assertEqual(reads,[(blobs.base+f'/object/authenticated/wallet-designs/studio-50-v1/native/ws-051-{item["sha256"]}.png',MAX_ASSET)])

 def test_load_legacy_catalog_rejects_non_320x200_entries_and_accepts_valid_legacy(self):
  from designs import load_legacy_catalog, DEFAULT_LEGACY_CATALOG_PATH, DEFAULT_CATALOG_PATH
  legacy = load_legacy_catalog(DEFAULT_LEGACY_CATALOG_PATH)
  self.assertEqual(len(legacy['designs']), 56)
  with self.assertRaises(ValueError):
   load_legacy_catalog(DEFAULT_CATALOG_PATH)

class NativeAssetHttpTests(unittest.TestCase):
 setUp=fixtures.PaidDesignTests.setUp
 tearDown=fixtures.PaidDesignTests.tearDown
 order=fixtures.PaidDesignTests.order
 webhook=fixtures.PaidDesignTests.webhook
 configure=fixtures.PaidDesignTests.configure
 activate_order=fixtures.PaidDesignTests.activate_order
 def test_authenticated_catalog_honours_the_native_capability_header(self):
  self.configure();session=self.activate_order();token=session['access_token']
  data=png_at(1536,969);legacy_data=png()
  catalog=fixture_catalog(legacy_data)
  catalog['designs']=[dict(id=f'ws-{i:03}',filename=f'ws-{i:03}.png',title=f'Nativo {i}',sha256=hashlib.sha256(data).hexdigest(),bytes=len(data),width=1536,height=969) for i in range(1,57)]
  legacy=fixture_catalog(legacy_data);legacy['designs']+= [dict(legacy['designs'][0],id=f'ws-{i:03}',filename=f'ws-{i:03}.png') for i in range(51,57)]
  native_path=pathlib.Path(self.tmp.name)/'native-catalog.json';native_path.write_text(json.dumps(catalog),encoding='utf-8')
  legacy_path=pathlib.Path(self.tmp.name)/'legacy-catalog.json';legacy_path.write_text(json.dumps(legacy),encoding='utf-8')
  self.store.design_catalog=DesignCatalog({'WALLET_DESIGN_CATALOG':str(native_path),'WALLET_DESIGN_LEGACY_CATALOG':str(legacy_path)})
  class Blobs:
   base='https://fixture.supabase.co/storage/v1';headers={}
   def request(self,url,method,headers,body=None,maximum=0):
    if '/bucket/' in url:return 200,b'{"public":false}'
    return 200,data if '/native/' in url else legacy_data
  self.store.design_catalog.blobs=lambda:Blobs()
  server=Server(('127.0.0.1',0),self.store);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
  def request(path='/v1/designs',capability=None):
   headers={'Authorization':'Bearer '+token}
   if capability is not None:headers[EXPANDED_CATALOG_HEADER]=capability
   req=urllib.request.Request(f'http://127.0.0.1:{server.server_port}'+path,headers=headers)
   with urllib.request.urlopen(req,timeout=5) as response:
    if path!='/v1/designs':self.assertEqual(response.headers['Content-Type'],'image/png')
    return response.status,response.read()
  try:
   self.assertEqual(json.loads(request()[1])['designs'],legacy['designs'][:50])
   self.assertEqual(json.loads(request(capability=EXPANDED_CATALOG_CAPABILITY)[1])['designs'],legacy['designs'])
   self.assertEqual(json.loads(request(capability=NATIVE_2K_CAPABILITY)[1])['designs'],legacy['designs'])
   status,native_body=request(capability=NATIVE_VARIANT_CAPABILITY)
   native=json.loads(native_body)
   self.assertEqual(status,200);self.assertEqual(len(native['designs']),56);self.assertEqual(native['designs'],catalog['designs'])
   status,v2_body=request(capability=NATIVE_VARIANT_CAPABILITY)
   v2=json.loads(v2_body)
   self.assertEqual(status,200);self.assertEqual(len(v2['designs']),56)
   self.assertEqual(v2['designs'],catalog['designs'])
   self.assertTrue(all((item['width'],item['height'])==(1536,969) for item in v2['designs']))
   self.assertEqual(json.loads(request(capability='unknown-future-value')[1])['designs'],legacy['designs'][:50])
   for item in catalog['designs']:
    with self.subTest(id=item['id']):
     self.assertEqual(request(path='/v1/designs/'+item['id'],capability=NATIVE_2K_CAPABILITY),(200,legacy_data))
     self.assertEqual(request(path='/v1/designs/'+item['id'],capability=NATIVE_VARIANT_CAPABILITY),(200,data))
     self.assertEqual(request(path='/v1/designs/'+item['id'],capability=EXPANDED_CATALOG_CAPABILITY),(200,legacy_data))
     self.assertEqual(request(path='/v1/designs/'+item['id']+'/thumbnail',capability=NATIVE_VARIANT_CAPABILITY),(200,legacy_data))
  finally:server.shutdown();server.server_close();thread.join()

 def test_verify_asset_checks_dimensions_and_size_limits_for_both_profiles(self):
  legacy_data=png()
  legacy_item=dict(id='ws-001',filename='ws-001.png',title='Legacy',sha256=hashlib.sha256(legacy_data).hexdigest(),bytes=len(legacy_data),width=320,height=200)
  from designs import verify_asset
  verify_asset(legacy_item,legacy_data)
  native_data=png_at(1536,969)
  native_item=dict(id='ws-001',filename='ws-001.png',title='Native',sha256=hashlib.sha256(native_data).hexdigest(),bytes=len(native_data),width=1536,height=969)
  verify_asset(native_item,native_data)
  with self.assertRaises(ValueError):verify_asset(native_item,legacy_data)
  with self.assertRaises(ValueError):verify_asset(legacy_item,native_data)

 def test_sync_manifest_requires_all_fifty_six_assets_and_updates_dimensions_and_hashes(self):
  from upload_designs import sync_manifest
  with tempfile.TemporaryDirectory() as tmp_dir:
   tmp_path = pathlib.Path(tmp_dir)
   catalog_file = tmp_path / 'catalog.json'
   designs = [dict(id=f'ws-{i:03}',filename=f'ws-{i:03}.png',title=f'Design {i}',sha256='0'*64,bytes=100,width=320,height=200) for i in range(1, 57)]
   catalog_file.write_text(json.dumps(dict(schema=1,pack_id='studio-50-v1',title='56 diseños',designs=designs)),encoding='utf-8')
   assets_dir = tmp_path / 'designs'
   assets_dir.mkdir()
   with self.assertRaises(ValueError):
    sync_manifest(assets_dir, catalog_file)
   sample_data = png_at(1536, 969)
   for i in range(1, 57):
    (assets_dir / f'ws-{i:03}.png').write_bytes(sample_data)
   count, changed = sync_manifest(assets_dir, catalog_file)
   self.assertEqual(count, 56)
   self.assertTrue(changed)
   updated = json.loads(catalog_file.read_text(encoding='utf-8'))
   self.assertTrue(all(item['width'] == 1536 and item['height'] == 969 and item['bytes'] == len(sample_data) for item in updated['designs']))
   count2, changed2 = sync_manifest(assets_dir, catalog_file)
   self.assertEqual(count2, 56)
   self.assertFalse(changed2)

class ManifestSafetyTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
  self.root=pathlib.Path(self.tmp.name);self.path=self.root/'catalog.json'
  self.data=png_at(1536,969)
  self.catalog=dict(schema=1,pack_id='studio-50-v1',title='56 diseños',designs=[dict(id=f'ws-{i:03}',filename=f'ws-{i:03}.png',title=f'Diseño {i}',sha256='0'*64,bytes=100,width=1536,height=969) for i in range(1,57)])
  self.path.write_text(json.dumps(self.catalog),encoding='utf-8')
 def test_missing_legacy_catalog_does_not_expose_native_catalog_to_old_clients(self):
  service=DesignCatalog({'WALLET_DESIGN_CATALOG':str(self.path),'WALLET_DESIGN_LEGACY_CATALOG':str(self.root/'missing.json')})
  with self.assertRaises(ValueError):service.catalog()
  self.assertEqual(len(service.catalog(NATIVE_VARIANT_CAPABILITY)['designs']),56)
 def test_custom_native_catalog_does_not_override_default_legacy_catalog(self):
  service=DesignCatalog({'WALLET_DESIGN_CATALOG':str(self.path)})
  self.assertTrue(all(item['width']==320 for item in service.catalog()['designs']))
  self.assertTrue(all(item['width']==320 for item in service.catalog(NATIVE_2K_CAPABILITY)['designs']))
  self.assertTrue(all(item['width']==1536 for item in service.catalog(NATIVE_VARIANT_CAPABILITY)['designs']))
 def test_v2_selects_all_fifty_six_native_entries_not_fifty_legacy_entries(self):
  service=DesignCatalog({'WALLET_DESIGN_CATALOG':str(self.path)})
  result=catalog_for_client(service.catalog(NATIVE_VARIANT_CAPABILITY),NATIVE_VARIANT_CAPABILITY)
  self.assertEqual(len(result['designs']),56)
  self.assertEqual(result['designs'][0]['id'],'ws-001')
  self.assertEqual(result['designs'][-1]['id'],'ws-056')
  self.assertTrue(all(item['width']==1536 and item['height']==969 for item in result['designs']))
  self.assertTrue(all('thumbnail' not in item for item in result['designs']))
 def test_last_corrupt_asset_preserves_original_manifest_bytes(self):
  from upload_designs import sync_manifest
  assets=self.root/'assets';assets.mkdir()
  for i in range(1,57):(assets/f'ws-{i:03}.png').write_bytes(self.data)
  (assets/'ws-056.png').write_bytes(b'corrupt')
  before=self.path.read_bytes()
  with self.assertRaises(ValueError):sync_manifest(assets,self.path)
  self.assertEqual(self.path.read_bytes(),before)
 def test_path_traversal_catalog_is_rejected_without_mutation(self):
  from upload_designs import sync_manifest
  self.catalog['designs'][0]['filename']='../outside.png'
  self.path.write_text(json.dumps(self.catalog),encoding='utf-8');before=self.path.read_bytes()
  with self.assertRaises(ValueError):sync_manifest(self.root,self.path)
  self.assertEqual(self.path.read_bytes(),before)
 def test_new_native_hash_uses_a_different_immutable_object_key(self):
  from designs import object_key
  item=self.catalog['designs'][0];other=dict(item,sha256='1'*64)
  self.assertNotEqual(object_key(item),object_key(other))
  self.assertEqual(object_key(dict(item,width=320,height=200)),'studio-50-v1/ws-001.png')
 def provision_fixture(self):
  from upload_designs import sync_manifest
  assets=self.root/'assets';assets.mkdir()
  for i in range(1,57):(assets/f'ws-{i:03}.png').write_bytes(self.data)
  sync_manifest(assets,self.path)
  return assets
 def test_native_provisioning_and_replacement_preserve_old_objects(self):
  from upload_designs import provision,sync_manifest
  assets=self.provision_fixture();objects={};uploads=[]
  def remote(url,method,headers,body=None,maximum=0):
   if '/bucket/' in url:return 200,json.dumps({'public':False,'file_size_limit':MAX_ASSET}).encode()
   key=url.split('/wallet-designs/',1)[1]
   if method=='POST':
    self.assertEqual(headers['x-upsert'],'false');uploads.append(key)
    if key in objects:return 409,b'{}'
    objects[key]=body;return 201,b'{}'
   return 200,objects[key]
  config={'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'}
  self.assertEqual(provision(config,assets,self.path,remote),56)
  previous=dict(objects)
  self.assertTrue(all('/native/' in key for key in uploads))
  (assets/'ws-001.png').write_bytes(png_at(1536,969,b'\x11\x22\x33'))
  sync_manifest(assets,self.path)
  self.assertEqual(provision(config,assets,self.path,remote,only_ids=['ws-001']),1)
  self.assertEqual(len(objects),57)
  self.assertTrue(all(objects[key]==body for key,body in previous.items()))
 def test_bucket_limit_failure_occurs_before_any_upload(self):
  from upload_designs import provision
  from remote import RemoteError
  assets=self.provision_fixture();calls=[]
  def remote(url,method,headers,body=None,maximum=0):
   calls.append(method)
   return 200,json.dumps({'public':False,'file_size_limit':len(self.data)-1}).encode()
  with self.assertRaisesRegex(RemoteError,'size_limit_too_small'):
   provision({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'},assets,self.path,remote)
  self.assertEqual(calls,['GET'])

if __name__=='__main__':unittest.main()
