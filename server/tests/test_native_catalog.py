"""Native 2K (1536x969) catalogue assets up to 4 MiB with full 320x200 legacy compatibility."""
import hashlib,json,pathlib,struct,sys,tempfile,threading,unittest,urllib.request,zlib
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from app import Server
from designs import (ASSET_LIMITS,DesignCatalog,EXPANDED_CATALOG_CAPABILITY,EXPANDED_CATALOG_HEADER,LEGACY_ASSET,LEGACY_SIZE,
                     MAX_ASSET,NATIVE_2K_CAPABILITY,NATIVE_SIZE,catalog_for_client,load_catalog,validate_png)
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
 def test_load_catalog_accepts_native_entries_only_after_the_legacy_prefix(self):
  data=png();base=fixture_catalog(data)
  self.assertEqual(len(self.load(base)['designs']),50)
  extended=fixture_catalog(data);extended['designs']+=native_metadata(data)
  self.assertEqual(len(self.load(extended)['designs']),56)
  heavy=json.loads(json.dumps(extended));heavy['designs'][50]['bytes']=MAX_ASSET+1
  with self.assertRaises(ValueError):self.load(heavy)
  moved=json.loads(json.dumps(extended));moved['designs'][49]=dict(moved['designs'][49],width=1536,height=969,bytes=1024)
  with self.assertRaises(ValueError):self.load(moved)
  fat=json.loads(json.dumps(base));fat['designs'][0]['bytes']=LEGACY_ASSET+1
  with self.assertRaises(ValueError):self.load(fat)
 def test_catalog_for_client_serves_native_assets_only_for_the_native_capability(self):
  data=png();catalog=fixture_catalog(data);catalog['designs']+=native_metadata(data,count=3)
  legacy=catalog_for_client(catalog,None)
  self.assertEqual(legacy['designs'],catalog['designs'][:50]);self.assertEqual(legacy['title'],'50 diseños')
  expanded=catalog_for_client(catalog,EXPANDED_CATALOG_CAPABILITY)
  self.assertEqual(expanded['designs'],catalog['designs'][:50]);self.assertEqual(expanded['title'],'50 diseños')
  native=catalog_for_client(catalog,NATIVE_2K_CAPABILITY)
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
  self.assertEqual(reads,[(blobs.base+'/object/authenticated/wallet-designs/studio-50-v1/ws-051.png',MAX_ASSET)])

class NativeAssetHttpTests(unittest.TestCase):
 setUp=fixtures.PaidDesignTests.setUp
 tearDown=fixtures.PaidDesignTests.tearDown
 order=fixtures.PaidDesignTests.order
 webhook=fixtures.PaidDesignTests.webhook
 configure=fixtures.PaidDesignTests.configure
 activate_order=fixtures.PaidDesignTests.activate_order
 def test_authenticated_catalog_honours_the_native_capability_header(self):
  self.configure();session=self.activate_order();token=session['access_token']
  data=png();catalog=fixture_catalog(data);catalog['designs']+=native_metadata(data)
  self.store.design_catalog.catalog=lambda:catalog;self.store.design_catalog.read=lambda item:data
  server=Server(('127.0.0.1',0),self.store);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
  def request(capability=None):
   headers={'Authorization':'Bearer '+token}
   if capability is not None:headers[EXPANDED_CATALOG_HEADER]=capability
   with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.server_port}/v1/designs',headers=headers),timeout=5) as response:return response.status,json.loads(response.read())
  try:
   self.assertEqual(request()[1]['designs'],catalog['designs'][:50])
   self.assertEqual(request(EXPANDED_CATALOG_CAPABILITY)[1]['designs'],catalog['designs'][:50])
   status,native=request(NATIVE_2K_CAPABILITY)
   self.assertEqual(status,200);self.assertEqual(len(native['designs']),56);self.assertEqual(native['designs'],catalog['designs'])
   self.assertEqual(request('unknown-future-value')[1]['designs'],catalog['designs'][:50])
  finally:server.shutdown();server.server_close();thread.join()

if __name__=='__main__':unittest.main()
