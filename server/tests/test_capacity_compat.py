"""Do not regress the bounded overload handling already deployed in 0.6.10."""
import concurrent.futures,os,pathlib,socket,sys,threading,unittest,urllib.request,urllib.error
from unittest.mock import patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from app import Server,Handler

class CapacityCompatibility(unittest.TestCase):
 def test_capacity_one_returns_retryable_503_and_recovers(self):
  entered=threading.Event();release=threading.Event();original=Handler.do_GET
  def get(handler):
   if handler.path=='/slow':entered.set();release.wait(4);return handler.reply(200,{'fixture':True})
   return original(handler)
  with patch.dict(os.environ,{'WALLET_MAX_CONCURRENT_REQUESTS':'1'}),patch.object(Handler,'do_GET',get):
   server=Server(('127.0.0.1',0),object());thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
   def request(path,method='GET'):
    return urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.server_port}'+path,headers={'Connection':'close'},method=method),timeout=5)
   try:
    with concurrent.futures.ThreadPoolExecutor(1) as pool:
     first=pool.submit(request,'/slow');self.assertTrue(entered.wait(2))
     with request('/health') as response:self.assertEqual(response.status,200)
     for method in ['POST','HEAD']:
      with self.assertRaises(urllib.error.HTTPError) as rejected:request('/health',method)
      self.assertEqual(rejected.exception.code,503)
     for path in ['/health?probe=1','/health/','/v1/designs']:
      with self.assertRaises(urllib.error.HTTPError) as rejected:request(path)
      self.assertEqual(rejected.exception.code,503)
     with self.assertRaises(urllib.error.HTTPError) as error:request('/v1/designs')
     self.assertEqual(error.exception.code,503)
     self.assertEqual(error.exception.headers['Retry-After'],'1')
     self.assertLess(len(error.exception.read()),128)
     release.set();first.result().close()
    with request('/health') as response:self.assertEqual(response.status,200)
   finally:release.set();server.shutdown();server.server_close();thread.join()
 def test_idle_keepalive_does_not_hold_the_only_worker(self):
  with patch.dict(os.environ,{'WALLET_MAX_CONCURRENT_REQUESTS':'1'}):
   server=Server(('127.0.0.1',0),object());thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
   try:
    # Leave the client socket open, as a reverse proxy's connection pool would.
    with socket.create_connection(server.server_address,timeout=2) as client:
     client.sendall(b'GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n')
     response=bytearray()
     while True:
      part=client.recv(4096)
      if not part:break
      response.extend(part)
     self.assertIn(b'Connection: close',response)
     # A protected path must reach the handler, not the busy 503 fast path.
     server.store=type('Store',(),{'subject':lambda *args,**kwargs:None})()
     with self.assertRaises(urllib.error.HTTPError) as error:
      urllib.request.urlopen(f'http://127.0.0.1:{server.server_port}/v1/designs',timeout=2)
     self.assertEqual(error.exception.code,401)
   finally:server.shutdown();server.server_close();thread.join()
 def test_container_includes_both_catalogs(self):
  dockerfile=(pathlib.Path(__file__).resolve().parents[1]/'Dockerfile').read_text(encoding='utf-8')
  self.assertIn('COPY designs.py design-catalog.json design-catalog.legacy.json /service/',dockerfile)

if __name__=='__main__':unittest.main()
