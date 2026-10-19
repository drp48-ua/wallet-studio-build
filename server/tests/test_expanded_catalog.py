"""Append-only catalogue expansion with authenticated legacy compatibility."""
import copy,hashlib,json,os,pathlib,sys,tempfile,threading,unittest,urllib.error,urllib.request
from unittest.mock import patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from app import Server
from designs import load_catalog,verify_asset,catalog_for_client,EXPANDED_CATALOG_HEADER,EXPANDED_CATALOG_CAPABILITY,MAX_DESIGNS
import test_designs as fixtures
from test_designs import fixture_catalog,png
from upload_designs import provision

CATALOG=pathlib.Path(__file__).resolve().parents[1]/'design-catalog.json'
EXTRAS=[
 ('sakura-fuji-plata.png','Sakura Fuji plata'),
 ('telarana-roja-oro.png','Telaraña roja y oro'),
 ('marmol-blanco-oro.png','Mármol blanco y oro'),
 ('topografia-esmeralda.png','Topografía esmeralda'),
 ('gran-ola-japonesa.png','La gran ola japonesa'),
 ('clasica-negra-oro.png','Clásica negra y oro'),
]

class ExpandedCatalogTests(unittest.TestCase):
 setUp=fixtures.PaidDesignTests.setUp
 tearDown=fixtures.PaidDesignTests.tearDown
 configure=fixtures.PaidDesignTests.configure
 activate_order=fixtures.PaidDesignTests.activate_order
 order=fixtures.PaidDesignTests.order
 webhook=fixtures.PaidDesignTests.webhook

 def test_existing_fifty_entries_are_unchanged_and_six_have_immutable_new_ids(self):
  catalog=load_catalog(CATALOG)
  catalog['designs']=[{k:v for k,v in item.items() if k!='native'} for item in catalog['designs']]
  canonical=json.dumps(catalog['designs'][:50],sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()
  self.assertEqual(hashlib.sha256(canonical).hexdigest(),'2faf24ec90345b325a6c0a32d471172826b6484bdcd61f57ec21d31d0fa1ed40')
  self.assertEqual(len(catalog['designs']),56)
  for index,(item,(_,title)) in enumerate(zip(catalog['designs'][50:],EXTRAS),51):
   self.assertEqual((item['id'],item['filename'],item['title']),(f'ws-{index:03}',f'ws-{index:03}.png',title))
  legacy=catalog_for_client(catalog,None);expanded=catalog_for_client(catalog,EXPANDED_CATALOG_CAPABILITY)
  self.assertEqual(legacy['designs'],catalog['designs'][:50]);self.assertEqual(legacy['title'],'50 diseños')
  self.assertEqual(expanded['designs'],catalog['designs']);self.assertEqual(expanded['title'],'56 diseños')
  self.assertEqual(catalog_for_client(catalog,'unknown-future-value')['designs'],legacy['designs'])
  self.assertEqual(len(catalog['designs']),56,'Legacy response must not truncate shared state')

 def test_expanded_catalog_validation_rejects_missing_duplicate_reordered_and_excessive_entries(self):
  original=load_catalog(CATALOG)
  with tempfile.TemporaryDirectory() as directory:
   path=pathlib.Path(directory)/'catalog.json'
   for count in (50,56,MAX_DESIGNS):
    data=fixture_catalog(png());template=data['designs'][0]
    data['designs']=[dict(template,id=f'ws-{i:03}',filename=f'ws-{i:03}.png') for i in range(1,count+1)]
    path.write_text(json.dumps(data),encoding='utf-8');self.assertEqual(len(load_catalog(path)['designs']),count)
   malformed=[]
   missing=copy.deepcopy(original);missing['designs'].pop(50);malformed.append(missing)
   duplicate=copy.deepcopy(original);duplicate['designs'][51]=duplicate['designs'][50];malformed.append(duplicate)
   reordered=copy.deepcopy(original);reordered['designs'][50:52]=reordered['designs'][51:49:-1];malformed.append(reordered)
   excessive=copy.deepcopy(original);excessive['designs']*=2;malformed.append(excessive)
   for value in malformed:
    path.write_text(json.dumps(value),encoding='utf-8')
    with self.assertRaises(ValueError):load_catalog(path)

 def test_authenticated_legacy_fifty_expanded_fifty_six_download_and_revocation(self):
  self.configure();session=self.activate_order();token=session['access_token'];user=session['user_id']
  data=png();catalog=fixture_catalog(data);template=catalog['designs'][0]
  catalog['designs']=[dict(template,id=f'ws-{i:03}',filename=f'ws-{i:03}.png') for i in range(1,57)]
  self.store.design_catalog.catalog=lambda:catalog;self.store.design_catalog.read=lambda item:data
  server=Server(('127.0.0.1',0),self.store);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
  def request(path,token=token,capability=None):
   headers={} if token is None else {'Authorization':'Bearer '+token}
   if capability is not None:headers[EXPANDED_CATALOG_HEADER]=capability
   try:
    with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.server_port}'+path,headers=headers),timeout=5) as response:return response.status,response.read()
   except urllib.error.HTTPError as error:return error.code,error.read()
  try:
   self.assertEqual(request('/v1/designs',None,EXPANDED_CATALOG_CAPABILITY)[0],401)
   status,legacy=request('/v1/designs');self.assertEqual(status,200);legacy=json.loads(legacy)
   status,expanded=request('/v1/designs',capability=EXPANDED_CATALOG_CAPABILITY);self.assertEqual(status,200);expanded=json.loads(expanded)
   self.assertEqual(legacy['designs'],catalog['designs'][:50]);self.assertEqual(len(expanded['designs']),56)
   self.assertEqual(expanded['designs'][:50],legacy['designs'])
   for item in expanded['designs'][50:]:
    status,body=request('/v1/designs/'+item['id']);self.assertEqual(status,200);verify_asset(item,body)
   with patch.object(self.auth,'pack_entitled',return_value=False),patch.object(self.auth,'subject',return_value=user):
    status,body=request('/v1/designs',capability=EXPANDED_CATALOG_CAPABILITY)
    self.assertEqual(status,200);self.assertEqual(json.loads(body)['designs'],[])
    self.assertEqual(request('/v1/designs/ws-056')[0],403)
   self.auth.revoke(user,'1'*64)
   self.assertEqual(request('/v1/designs',capability=EXPANDED_CATALOG_CAPABILITY)[0],401)
   self.assertEqual(request('/v1/designs/ws-056')[0],401)
  finally:server.shutdown();server.server_close();thread.join()

 @unittest.skipUnless(os.environ.get('WALLET_STUDIO_EXTRA_DESIGNS_DIR'),'private cropped source directory not supplied')
 def test_six_actual_cropped_assets_match_catalog_hash_bytes_dimensions_and_png_integrity(self):
  root=pathlib.Path(os.environ['WALLET_STUDIO_EXTRA_DESIGNS_DIR'])
  items=load_catalog(CATALOG)['designs'][50:]
  self.assertEqual(len(items),len(EXTRAS))
  for item,(filename,_) in zip(items,EXTRAS):
   with self.subTest(id=item['id']):verify_asset(item,(root/filename).read_bytes())

 def test_subset_provision_uses_only_selected_files_and_never_overwrites_conflicting_remote_asset(self):
  data=png();catalog=fixture_catalog(data);template=catalog['designs'][0]
  catalog['designs']=[dict(template,id=f'ws-{i:03}',filename=f'ws-{i:03}.png') for i in range(1,57)]
  root=pathlib.Path(self.tmp.name);catalog_path=root/'catalog.json';catalog_path.write_text(json.dumps(catalog),encoding='utf-8')
  for ident in ('ws-051','ws-056'):(root/(ident+'.png')).write_bytes(data)
  objects={'ws-001.png':b'old-object-must-stay'};posts=[]
  def remote(url,method,headers,body=None,maximum=0):
   if url.endswith('/bucket/wallet-designs'):return 200,b'{"public":false}'
   key=url.rsplit('/',1)[-1]
   if method=='POST':
    posts.append((key,headers.get('x-upsert')))
    if key in objects:return 409,b'{}'
    objects[key]=body;return 201,b'{}'
   return 200,objects[key]
  config={'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'}
  self.assertEqual(provision(config,root,catalog_path,remote,only_ids=['ws-051','ws-056']),2)
  self.assertEqual(objects['ws-001.png'],b'old-object-must-stay');self.assertEqual({key for key,_ in posts},{'ws-051.png','ws-056.png'})
  self.assertTrue(all(upsert=='false' for _,upsert in posts))
  self.assertEqual(provision(config,root,catalog_path,remote,only_ids=['ws-051','ws-056']),2)
  objects['ws-056.png']=b'existing-conflict'
  with self.assertRaises(ValueError):provision(config,root,catalog_path,remote,only_ids=['ws-051','ws-056'])
  self.assertEqual(objects['ws-056.png'],b'existing-conflict')

 def test_subset_provision_rejects_unknown_duplicate_and_corrupt_selected_files_before_any_remote_call(self):
  data=png();catalog=fixture_catalog(data);root=pathlib.Path(self.tmp.name);catalog_path=root/'catalog.json';catalog_path.write_text(json.dumps(catalog),encoding='utf-8')
  (root/'ws-001.png').write_bytes(data);(root/'ws-002.png').write_bytes(b'bad-second-file')
  calls=[]
  def remote(*args,**kwargs):calls.append(args);raise AssertionError('Preflight must finish before remote writes')
  config={'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'}
  for selection in ([],['ws-999'],['ws-001','ws-001'],['ws-001','ws-002']):
   with self.subTest(selection=selection),self.assertRaises(ValueError):provision(config,root,catalog_path,remote,only_ids=selection)
  self.assertEqual(calls,[])
