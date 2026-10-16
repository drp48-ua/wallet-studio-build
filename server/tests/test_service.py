import sys,pathlib,unittest,tempfile,threading,json,urllib.request,urllib.error,hashlib,zipfile,io,os
from unittest.mock import patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from app import Store,Server
class ServiceTests(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  cls.environment=patch.dict(os.environ,{'WALLET_STORAGE_MODE':'local','RENDER':''});cls.environment.start();cls.addClassCleanup(cls.environment.stop)
  cls.scratch=pathlib.Path(__file__).resolve().parents[1]/'test-runtime';cls.scratch.mkdir(exist_ok=True);cls.tmp=tempfile.TemporaryDirectory(dir=cls.scratch);cls.store=Store(cls.tmp.name,True);cls.server=Server(('127.0.0.1',0),cls.store);cls.thread=threading.Thread(target=cls.server.serve_forever,daemon=True);cls.thread.start();cls.url='http://127.0.0.1:'+str(cls.server.server_port)
 @classmethod
 def tearDownClass(cls):cls.server.shutdown();cls.server.server_close();assert pathlib.Path(cls.tmp.name).resolve().is_relative_to(cls.scratch.resolve());cls.tmp.cleanup()
 def call(self,path,method='GET',body=None,token=None):
  headers={}
  if token:headers['Authorization']='Bearer '+token
  raw=json.dumps(body).encode() if isinstance(body,dict) else body
  try:
   with urllib.request.urlopen(urllib.request.Request(self.url+path,data=raw,headers=headers,method=method),timeout=10) as r:return r.status,r.read()
  except urllib.error.HTTPError as e:return e.code,e.read()
 def account(self,name):
  code,raw=self.call('/v1/auth/register','POST',{'email':name+'@example.test','password':'fixture-only-strong-password'});self.assertEqual(code,200);return json.loads(raw)['access_token']
 def bundle(self):
  out=io.BytesIO()
  with zipfile.ZipFile(out,'w') as z:z.writestr('fixture.json','{"synthetic":true}')
  data=out.getvalue();return hashlib.sha256(data).hexdigest(),data
 def test_owner_isolation_and_immutable_roundtrip(self):
  a=self.account('owner-a');b=self.account('owner-b');ident,data=self.bundle();self.assertEqual(self.call('/v1/bundles/'+ident,'PUT',data,a)[0],201);self.assertEqual(self.call('/v1/bundles/'+ident,'PUT',data,a)[0],200);self.assertEqual(self.call('/v1/bundles/'+ident,token=a),(200,data));self.assertEqual(self.call('/v1/bundles/'+ident,token=b)[0],404);self.assertEqual(json.loads(self.call('/v1/bundles',token=b)[1])['bundles'],[])
 def test_no_token_wrong_password_and_digest_are_rejected(self):
  token=self.account('bad-input');self.assertEqual(self.call('/v1/bundles')[0],401);self.assertEqual(self.call('/v1/auth/login','POST',{'email':'bad-input@example.test','password':'wrong'})[0],401);self.assertEqual(self.call('/v1/bundles/'+'0'*64,'PUT',b'not a zip',token)[0],400)
 def test_path_escape_zip_is_rejected(self):
  token=self.account('unsafe-zip');out=io.BytesIO()
  with zipfile.ZipFile(out,'w') as z:z.writestr('../escape.json','x')
  data=out.getvalue();self.assertEqual(self.call('/v1/bundles/'+hashlib.sha256(data).hexdigest(),'PUT',data,token)[0],400)
 def test_accounts_are_admin_provisioned_by_default(self):
  self.store.allow_registration=False
  try:self.assertEqual(self.call('/v1/auth/register','POST',{'email':'disabled@example.test','password':'fixture-only-password'})[0],403)
  finally:self.store.allow_registration=True
 def test_non_object_auth_body_is_rejected(self):
  self.assertEqual(self.call('/v1/auth/login','POST',b'[]')[0],400)
 def test_expired_session_is_rejected(self):
  token=self.account('expired')
  with self.store.connect() as db:db.execute('UPDATE sessions SET expires=0 WHERE digest=?',(hashlib.sha256(token.encode()).hexdigest(),))
  self.assertEqual(self.call('/v1/bundles',token=token)[0],401)
 def test_server_does_not_return_corrupted_object(self):
  token=self.account('corrupt');ident,data=self.bundle();self.assertEqual(self.call('/v1/bundles/'+ident,'PUT',data,token)[0],201)
  subject=self.store.subject(token);(self.store.blobs/subject/ident).write_bytes(b'corrupt fixture')
  self.assertEqual(self.call('/v1/bundles/'+ident,token=token)[0],503)
if __name__=='__main__':unittest.main()
