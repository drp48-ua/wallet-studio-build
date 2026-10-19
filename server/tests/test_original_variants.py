"""Original-resolution variants, compatibility and bounded overload handling."""
import copy,hashlib,json,os,pathlib,threading,unittest,urllib.request,urllib.error,concurrent.futures
from unittest.mock import patch
import sys
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from app import Server,Handler
from designs import *
from upload_designs import provision
import test_designs as fixtures
from test_designs import fixture_catalog,png
from test_native_catalog import png_at

class OriginalVariants(unittest.TestCase):
 setUp=fixtures.PaidDesignTests.setUp
 tearDown=fixtures.PaidDesignTests.tearDown
 configure=fixtures.PaidDesignTests.configure
 activate_order=fixtures.PaidDesignTests.activate_order
 order=fixtures.PaidDesignTests.order
 webhook=fixtures.PaidDesignTests.webhook
 def fixture(self):
  legacy=png();native=png_at(1536,969)
  catalog=fixture_catalog(native)
  for item in catalog['designs']:item.update(width=1536,height=969)
  return catalog,legacy,native
 def test_original_dimensions_and_hashes_are_exact_and_old_clients_keep_legacy(self):
  catalog,legacy,native=self.fixture();path=pathlib.Path(self.tmp.name)/'catalog.json';path.write_text(json.dumps(catalog))
  load_catalog(path)
  for capability in [None,EXPANDED_CATALOG_CAPABILITY,NATIVE_2K_CAPABILITY]:
   self.assertEqual(catalog_for_client(fixture_catalog(legacy),capability)['designs'][0],fixture_catalog(legacy)['designs'][0])
  item=catalog_for_client(catalog,NATIVE_VARIANT_CAPABILITY)['designs'][0]
  self.assertEqual((item['width'],item['height']),(1536,969));verify_asset(item,native);verify_asset(fixture_catalog(legacy)['designs'][0],legacy)
  with self.assertRaises(ValueError):verify_asset(item,legacy)
  for width,height in [(639,460),(736,399),(4097,460),(4096,4097)]:
   changed=copy.deepcopy(catalog);changed['designs'][0].update(width=width,height=height);path.write_text(json.dumps(changed))
   with self.assertRaises(ValueError):load_catalog(path)
 def test_authenticated_variant_and_thumbnail_use_matching_bytes_and_revocation_still_denies(self):
  self.configure();session=self.activate_order();catalog,legacy,native=self.fixture();seen=[]
  self.store.design_catalog.catalog=lambda capability=None:catalog if capability==NATIVE_VARIANT_CAPABILITY else fixture_catalog(legacy)
  def read(item,capability=None):seen.append(item['width']==1536);return native if item['width']==1536 else legacy
  self.store.design_catalog.read=read
  server=Server(('127.0.0.1',0),self.store);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
  def get(path,capability,token=session['access_token']):
   request=urllib.request.Request(f'http://127.0.0.1:{server.server_port}'+path,headers={'Authorization':'Bearer '+token,EXPANDED_CATALOG_HEADER:capability,'Connection':'close'})
   with urllib.request.urlopen(request,timeout=5) as response:return response.headers,response.read()
  try:
   for capability,expected in [(NATIVE_2K_CAPABILITY,legacy),(NATIVE_VARIANT_CAPABILITY,native)]:
    headers,body=get('/v1/designs/ws-001',capability);self.assertEqual(body,expected);self.assertEqual(headers['Content-Type'],'image/png')
   self.assertEqual(get('/v1/designs/ws-001/thumbnail',NATIVE_VARIANT_CAPABILITY)[1],legacy)
   self.auth.revoke(session['user_id'],'1'*64)
   with self.assertRaises(urllib.error.HTTPError) as error:get('/v1/designs/ws-001',NATIVE_VARIANT_CAPABILITY)
   self.assertEqual(error.exception.code,401)
  finally:server.shutdown();server.server_close();thread.join()
 def test_native_provision_never_overwrites_legacy_and_validates_all_before_network(self):
  catalog,legacy,native=self.fixture();root=pathlib.Path(self.tmp.name);source=root/'originals';source.mkdir();(source/'ws-001.png').write_bytes(native)
  path=root/'catalog.json';path.write_text(json.dumps(catalog));calls=[]
  def request(url,method,headers,body=None,maximum=0):
   calls.append((url,method))
   if '/bucket/' in url:return 200,b'{"public":false}'
   self.assertIn('/studio-50-v1/native/ws-001-'+catalog['designs'][0]['sha256']+'.png',url)
   return (201,b'{}') if method=='POST' else (200,native)
  self.assertEqual(provision({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'},source,path,request=request,only_ids=['ws-001'],native=True),1)
  calls.clear();(source/'ws-001.png').write_bytes(legacy)
  with self.assertRaises(ValueError):provision({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'fixture-service'},source,path,request=request,only_ids=['ws-001'],native=True)
  self.assertEqual(calls,[])

class Capacity(unittest.TestCase):
 def test_capacity_one_returns_bounded_retryable_503_and_recovers(self):
  entered=threading.Event();release=threading.Event();original=Handler.do_GET
  def get(handler):
   if handler.path=='/slow':entered.set();release.wait(4);return handler.reply(200,{'fixture':True})
   return original(handler)
  with patch.dict(os.environ,{'WALLET_MAX_CONCURRENT_REQUESTS':'1'}),patch.object(Handler,'do_GET',get):
   server=Server(('127.0.0.1',0),object());thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
   def request(path):
    return urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.server_port}'+path,headers={'Connection':'close'}),timeout=5)
   try:
    with concurrent.futures.ThreadPoolExecutor(1) as pool:
     first=pool.submit(request,'/slow');self.assertTrue(entered.wait(2))
     with request('/health') as response:self.assertEqual(response.status,200)
     with self.assertRaises(urllib.error.HTTPError) as error:request('/v1/designs')
     self.assertEqual(error.exception.code,503);self.assertEqual(error.exception.headers['Retry-After'],'1');self.assertLess(len(error.exception.read()),128)
     release.set();first.result().close()
    with request('/health') as response:self.assertEqual(response.status,200)
   finally:release.set();server.shutdown();server.server_close();thread.join()
