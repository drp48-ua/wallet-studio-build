"""Private design-cache contract: exact PNGs, bounded RAM, current route authorization."""
import concurrent.futures,hashlib,json,pathlib,sys,threading,unittest,urllib.error,urllib.request
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import test_designs as fixtures
from test_native_catalog import png_at,native_metadata
from app import Server
from designs import DesignCatalog,DESIGN_CACHE_BYTES,DESIGN_CACHE_ENTRIES,NATIVE_VARIANT_CAPABILITY,EXPANDED_CATALOG_HEADER,validate_png


class BlobSpy:
 base='https://fixture.supabase.co/storage/v1'
 headers={'Authorization':'Bearer synthetic-service'}
 def __init__(self,data):
  self.data=data;self.public=False;self.calls=[];self.object_bytes=0;self.started=None;self.release=None
 def request(self,url,method,headers,body=None,maximum=0):
  self.calls.append((url,method,maximum))
  if '/bucket/' in url:return 200,json.dumps({'public':self.public}).encode()
  if '/object/authenticated/wallet-designs/' not in url:raise AssertionError('private authenticated object route required')
  if self.started is not None:
   self.started.set()
   if not self.release.wait(5):raise AssertionError('synthetic cache concurrency timed out')
  self.object_bytes+=len(self.data);return 200,self.data


class DesignCacheTests(unittest.TestCase):
 def make(self,**limits):
  data=png_at(1536,969);item=native_metadata(data)[0]
  service=DesignCatalog({'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'synthetic-service'},**limits)
  spy=BlobSpy(data);service.blobs=lambda:spy
  return service,spy,item,data
 def test_second_native_read_preserves_png_and_makes_zero_supabase_calls(self):
  service,spy,item,data=self.make()
  self.assertEqual(service.read(item),data);self.assertEqual(len(spy.calls),2)
  transferred=spy.object_bytes
  for _ in range(3):
   repeated=service.read(dict(item));self.assertEqual(repeated,data)
   self.assertEqual(hashlib.sha256(repeated).hexdigest(),item['sha256']);self.assertEqual(validate_png(repeated),(1536,969))
  self.assertEqual(len(spy.calls),2);self.assertEqual(spy.object_bytes,transferred);self.assertEqual(transferred,len(data))
 def test_cold_concurrent_readers_share_one_bucket_and_one_asset_get(self):
  service,spy,item,data=self.make();spy.started=threading.Event();spy.release=threading.Event();barrier=threading.Barrier(7)
  def read():barrier.wait(5);return service.read(item)
  with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
   futures=[pool.submit(read) for _ in range(6)];barrier.wait(5)
   try:self.assertTrue(spy.started.wait(5))
   finally:spy.release.set()
   self.assertEqual([future.result(10) for future in futures],[data]*6)
  self.assertEqual(len(spy.calls),2);self.assertEqual(spy.object_bytes,len(data))
 def test_current_metadata_and_origin_are_bound_and_invalid_updates_do_not_poison_cache(self):
  service,spy,item,data=self.make();service.read(item)
  for field,value in [('sha256','0'*64),('bytes',len(data)+1),('width',320),('height',200)]:
   changed=dict(item,**{field:value})
   with self.assertRaises(ValueError):service.read(changed)
   calls=len(spy.calls);self.assertEqual(service.read(item),data);self.assertEqual(len(spy.calls),calls)
  updated=png_at(1536,969,b'\x66\x77\x88');changed=dict(item,bytes=len(updated),sha256=hashlib.sha256(updated).hexdigest());spy.data=updated
  self.assertEqual(service.read(changed),updated);calls=len(spy.calls)
  self.assertEqual(service.read(changed),updated);self.assertEqual(len(spy.calls),calls)
  service.config['SUPABASE_URL']='https://other-fixture.supabase.co';self.assertEqual(service.read(changed),updated);self.assertEqual(len(spy.calls),calls+2)
 def test_corrupt_memory_entry_is_reverified_and_failed_remote_reads_are_not_cached(self):
  service,spy,item,data=self.make();service.read(item);key=next(iter(service._cache));service._cache[key]=b'x'+data[1:]
  self.assertEqual(service.read(item),data);self.assertEqual(len(spy.calls),4);self.assertEqual(service._cache_size,len(data))
  for public,payload in [(True,data),(False,b'corrupt remote PNG')]:
   cold,remote,metadata,_=self.make();remote.public=public;remote.data=payload
   for _ in range(2):
    with self.assertRaises(ValueError):cold.read(metadata)
   self.assertEqual(len(cold._cache),0);self.assertEqual(cold._cache_size,0)
   self.assertEqual(len(remote.calls),2 if public else 4)
 def test_lru_entry_and_byte_limits_and_oversize_bypass_are_bounded(self):
  service,spy,first,data=self.make(cache_entries=2)
  second=dict(first,id='ws-052',filename='ws-052.png');third=dict(first,id='ws-053',filename='ws-053.png')
  for item in [first,second,first,third]:self.assertEqual(service.read(item),data)
  self.assertEqual(len(spy.calls),6);self.assertEqual(len(service._cache),2)
  service.read(first);self.assertEqual(len(spy.calls),6);service.read(second);self.assertEqual(len(spy.calls),8)
  limited,remote,item,_=self.make(cache_bytes=len(data));limited.read(item);limited.read(second)
  self.assertEqual(limited._cache_size,len(data));self.assertEqual(len(limited._cache),1);limited.read(item);self.assertEqual(len(remote.calls),6)
  for limits in [dict(cache_bytes=len(data)-1),dict(cache_bytes=0),dict(cache_entries=0)]:
   uncached,remote,item,_=self.make(**limits);self.assertEqual(uncached.read(item),data);self.assertEqual(uncached.read(item),data)
   self.assertEqual(len(remote.calls),4);self.assertEqual(uncached._cache_size,0);self.assertFalse(uncached._cache)
  for limits in [dict(cache_bytes=-1),dict(cache_bytes=DESIGN_CACHE_BYTES+1),dict(cache_entries=DESIGN_CACHE_ENTRIES+1),dict(cache_entries=True)]:
   with self.assertRaises(ValueError):self.make(**limits)


class DesignCacheHttpTests(unittest.TestCase):
 setUp=fixtures.PaidDesignTests.setUp
 tearDown=fixtures.PaidDesignTests.tearDown
 order=fixtures.PaidDesignTests.order
 webhook=fixtures.PaidDesignTests.webhook
 configure=fixtures.PaidDesignTests.configure
 activate_order=fixtures.PaidDesignTests.activate_order
 def test_real_route_rechecks_authorization_pack_and_catalog_before_warm_cache(self):
  self.configure();session=self.activate_order();data=png_at(1536,969)
  catalog=fixtures.fixture_catalog(data)
  catalog['designs']=[dict(native_metadata(data)[0],id=f'ws-{i:03}',filename=f'ws-{i:03}.png') for i in range(1,57)]
  path=pathlib.Path(self.tmp.name)/'native-cache-catalog.json';path.write_text(json.dumps(catalog),encoding='utf-8')
  service=DesignCatalog({'WALLET_DESIGN_CATALOG':str(path),'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'synthetic-service'})
  spy=BlobSpy(data);service.blobs=lambda:spy;self.store.design_catalog=service
  server=Server(('127.0.0.1',0),self.store);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
  def request(token):
   headers={EXPANDED_CATALOG_HEADER:NATIVE_VARIANT_CAPABILITY}
   if token is not None:headers['Authorization']='Bearer '+token
   try:
    with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.server_port}/v1/designs/ws-051',headers=headers),timeout=10) as response:return response.status,response.read()
   except urllib.error.HTTPError as error:return error.code,error.read()
  try:
   self.assertEqual(request(None)[0],401);self.assertFalse(spy.calls)
   self.assertEqual(request(session['access_token']),(200,data));self.assertEqual(len(spy.calls),2)
   self.assertEqual(request(session['access_token']),(200,data));self.assertEqual(len(spy.calls),2);self.assertEqual(spy.object_bytes,len(data))
   from unittest.mock import patch
   with patch.object(self.auth,'pack_entitled',return_value=False):self.assertEqual(request(session['access_token'])[0],403)
   self.assertEqual(request('invalid-fixture-token')[0],401);self.assertEqual(len(spy.calls),2)
   catalog['designs']=catalog['designs'][:50];path.write_text(json.dumps(catalog),encoding='utf-8')
   self.assertEqual(request(session['access_token'])[0],404);self.assertEqual(len(spy.calls),2)
   self.auth.revoke(session['user_id'],'1'*64);self.assertEqual(request(session['access_token'])[0],401);self.assertEqual(len(spy.calls),2)
  finally:server.shutdown();server.server_close();thread.join()
