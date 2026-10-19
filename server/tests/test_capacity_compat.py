"""Do not regress the bounded overload handling already deployed in 0.6.10."""
import concurrent.futures,os,pathlib,sys,threading,unittest,urllib.request,urllib.error
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
   def request(path):
    return urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.server_port}'+path,headers={'Connection':'close'}),timeout=5)
   try:
    with concurrent.futures.ThreadPoolExecutor(1) as pool:
     first=pool.submit(request,'/slow');self.assertTrue(entered.wait(2))
     with self.assertRaises(urllib.error.HTTPError) as error:request('/health')
     self.assertEqual(error.exception.code,503)
     self.assertEqual(error.exception.headers['Retry-After'],'1')
     self.assertLess(len(error.exception.read()),128)
     release.set();first.result().close()
    with request('/health') as response:self.assertEqual(response.status,200)
   finally:release.set();server.shutdown();server.server_close();thread.join()
 def test_container_includes_both_catalogs(self):
  dockerfile=(pathlib.Path(__file__).resolve().parents[1]/'Dockerfile').read_text(encoding='utf-8')
  self.assertIn('COPY designs.py design-catalog.json design-catalog.legacy.json /service/',dockerfile)

if __name__=='__main__':unittest.main()
