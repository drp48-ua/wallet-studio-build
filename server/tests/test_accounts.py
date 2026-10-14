import base64,concurrent.futures,hashlib,hmac,json,pathlib,sys,tempfile,time,unittest,threading,urllib.request,urllib.error,os
from unittest.mock import patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from app import Store,Server
from accounts import Accounts,AccountError

class AccountTests(unittest.TestCase):
 def setUp(self):
  self.environment=patch.dict(os.environ,{'WALLET_STORAGE_MODE':'local','RENDER':''});self.environment.start();self.addCleanup(self.environment.stop)
  self.tmp=tempfile.TemporaryDirectory();self.store=Store(self.tmp.name);self.mail=[]
  self.config={'WALLET_AUTH_SECRET':'s'*32,'SHOPIFY_SHOP_DOMAIN':'fixture.myshopify.com','SHOPIFY_PRODUCT_IDS':'123','SHOPIFY_WEBHOOK_SECRET':'fixture-secret'}
  self.auth=Accounts(self.store,self.config,lambda email,code:self.mail.append((email,code)));self.store.accounts=self.auth
 def tearDown(self):self.tmp.cleanup()
 def order(self,ident=1,email='buyer@example.test',product=123,status='paid',updated='2026-10-15T12:00:00Z',**extra):
  return dict(id=ident,email=email,updated_at=updated,financial_status=status,line_items=[dict(id=99,product_id=product,quantity=1)],**extra)
 def webhook(self,body,event='event-1',topic='orders/paid',signature=None):
  raw=json.dumps(body).encode();sig=signature or base64.b64encode(hmac.new(b'fixture-secret',raw,hashlib.sha256).digest()).decode()
  self.auth.webhook(raw,sig,'fixture.myshopify.com',topic,event)
 def activate(self):
  self.webhook(self.order());self.auth.start('buyer@example.test');return self.auth.activate('buyer@example.test',self.mail[-1][1],'strong-fixture-password','strong-fixture-password','1'*64,'127.0.0.1')
 def test_nonbuyer_unpaid_wrong_product_cannot_activate(self):
  for i,order in enumerate([self.order(status='pending'),self.order(product=999)]):self.webhook(order,str(i));self.auth.start('buyer@example.test')
  self.auth.start('unknown@example.test');self.assertEqual(self.mail,[])
  with self.assertRaises(AccountError):self.auth.activate('unknown@example.test','123456','strong-fixture-password','strong-fixture-password','1'*64,'x')
 def test_activation_requires_code_password_match_and_consumes_once(self):
  result=self.activate();self.assertEqual(self.auth.subject(result['access_token']),result['user_id'])
  with self.assertRaises(AccountError):self.auth.activate('buyer@example.test',self.mail[-1][1],'strong-fixture-password','strong-fixture-password','1'*64,'x')
 def test_code_expires_and_attempt_limit_persists(self):
  self.webhook(self.order());self.auth.start('buyer@example.test');code=self.mail[-1][1]
  for _ in range(5):
   with self.assertRaises(AccountError):self.auth.activate('buyer@example.test','000000' if code!='000000' else '000001','strong-fixture-password','strong-fixture-password','1'*64,'x')
  with self.assertRaises(AccountError):self.auth.activate('buyer@example.test',code,'strong-fixture-password','strong-fixture-password','1'*64,'x')
  with self.store.connect() as db:db.execute('UPDATE activation SET attempts=0,expires=0')
  with self.assertRaises(AccountError):self.auth.activate('buyer@example.test',code,'strong-fixture-password','strong-fixture-password','1'*64,'x')
 def test_three_devices_atomic_races_and_revoke(self):
  first=self.activate()
  def login(i):
   try:return self.auth.login('buyer@example.test','strong-fixture-password',f'{i:064x}','different-ip')
   except AccountError:return None
  with concurrent.futures.ThreadPoolExecutor(5) as pool:results=list(pool.map(login,range(2,7)))
  self.assertEqual(sum(x is not None for x in results),2);self.assertEqual(len(self.auth.devices(first['user_id'])),3)
  self.auth.login('buyer@example.test','strong-fixture-password','1'*64,'new-network-does-not-add-device')
  self.auth.revoke(first['user_id'],'1'*64);self.assertIsNone(self.auth.subject(first['access_token']));self.assertIsNotNone(login(7))
 def test_session_inactivity_expiry_and_purchase_revocation(self):
  first=self.activate();digest=hashlib.sha256(first['access_token'].encode()).hexdigest()
  with self.store.connect() as db:db.execute('UPDATE sessions SET last_seen=? WHERE digest=?',(int(time.time())-86400*31,digest))
  self.assertIsNone(self.auth.subject(first['access_token']))
  second=self.auth.login('buyer@example.test','strong-fixture-password','1'*64,'x')
  with self.store.connect() as db:db.execute('UPDATE sessions SET expires=0 WHERE digest=?',(hashlib.sha256(second['access_token'].encode()).hexdigest(),))
  self.assertIsNone(self.auth.subject(second['access_token']))
  second=self.auth.login('buyer@example.test','strong-fixture-password','1'*64,'x')
  self.webhook(self.order(updated='2026-10-15T12:01:00Z',status='refunded'),'refund','orders/updated')
  self.assertIsNone(self.auth.subject(second['access_token']))
 def test_bad_hmac_replay_and_stale_events_cannot_reinstate(self):
  with self.assertRaises(AccountError):self.webhook(self.order(),signature='bad')
  first=self.activate()
  refund=self.order(updated='2026-10-15T12:05:00Z',status='refunded')
  self.webhook(refund,'refund','orders/updated');self.webhook(refund,'refund','orders/updated')
  self.webhook(self.order(),'old-paid');self.assertIsNone(self.auth.subject(first['access_token']))
 def test_refund_only_licensed_line_revokes_even_other_products_remain_paid(self):
  first=self.activate();body=self.order(status='partially_refunded',updated='2026-10-15T12:05:00Z',refunds=[{'refund_line_items':[{'line_item_id':99,'quantity':1}]}])
  body['line_items'].append(dict(id=100,product_id=999,quantity=1));self.webhook(body,'partial','orders/updated');self.assertIsNone(self.auth.subject(first['access_token']))
 def test_missing_configuration_closed_and_legacy_tokens_not_accepted(self):
  auth=Accounts(self.store,{})
  with self.assertRaises(AccountError):auth.start('buyer@example.test')
  self.assertIsNone(auth.subject('a'*64))
 def test_admin_created_user_and_legacy_session_do_not_bypass_purchase(self):
  user=self.store.create_user('admin-created@example.test','strong-fixture-password')
  with self.assertRaises(AccountError):self.auth.login('admin-created@example.test','strong-fixture-password','1'*64,'x')
  with self.store.connect() as db:db.execute('INSERT INTO sessions(digest,user_id,expires) VALUES(?,?,?)',(hashlib.sha256(('a'*64).encode()).hexdigest(),user,int(time.time())+3600))
  self.assertIsNone(self.auth.subject('a'*64))
 def test_mismatched_password_does_not_consume_activation_code(self):
  self.webhook(self.order());self.auth.start('buyer@example.test');code=self.mail[-1][1]
  with self.assertRaises(AccountError):self.auth.activate('buyer@example.test',code,'strong-fixture-password','different-fixture-password','1'*64,'x')
  result=self.auth.activate('buyer@example.test',code,'strong-fixture-password','strong-fixture-password','1'*64,'x');self.assertIsNotNone(self.auth.subject(result['access_token']))
 def test_invalid_request_capacity_is_rejected_at_startup(self):
  for value in ('0','5','invalid'):
   with patch.dict(os.environ,{'WALLET_MAX_CONCURRENT_REQUESTS':value}):
    with self.assertRaises(ValueError):Server(('127.0.0.1',0),self.store)
 def test_password_rotation_cannot_race_old_password_into_fresh_token(self):
  self.activate();self.auth.start('buyer@example.test','recover');code=self.mail[-1][1]
  checked=threading.Event();recovered=threading.Event();original=self.store.verify_password;results={}
  def verify(email,password):
   result=original(email,password)
   if threading.current_thread().name=='stale-password-login':checked.set();recovered.wait(3)
   return result
  def old_login():
   try:results['old']=self.auth.login('buyer@example.test','strong-fixture-password','1'*64,'x')
   except Exception as error:results['old_error']=error
  def recovery():
   checked.wait(10)
   try:results['new']=self.auth.recover('buyer@example.test',code,'new-fixture-password','new-fixture-password','2'*64,True,'x')
   except Exception as error:results['new_error']=error
   finally:recovered.set()
  with patch.object(self.store,'verify_password',side_effect=verify):
   first=threading.Thread(target=old_login,name='stale-password-login');second=threading.Thread(target=recovery);first.start();second.start();first.join(15);second.join(15)
  self.assertFalse(first.is_alive() or second.is_alive());self.assertNotIn('old_error',results);self.assertNotIn('new_error',results)
  self.assertIsNone(self.auth.subject(results['old']['access_token']));self.assertIsNotNone(self.auth.subject(results['new']['access_token']))
 def test_verified_email_recovers_fourth_device_without_prior_vault_access(self):
  first=self.activate()
  for i in (2,3):self.auth.login('buyer@example.test','strong-fixture-password',str(i)*64,'x')
  with self.assertRaises(AccountError):self.auth.login('buyer@example.test','strong-fixture-password','4'*64,'x')
  with self.store.connect() as db:db.execute('DELETE FROM activation')
  self.auth.start('buyer@example.test','recover');code=self.mail[-1][1]
  with self.assertRaises(AccountError):self.auth.recover('buyer@example.test','000000' if code!='000000' else '000001','new-fixture-password','new-fixture-password','4'*64,True,'x')
  self.assertEqual(len(self.auth.devices(first['user_id'])),3)
  recovered=self.auth.recover('buyer@example.test',code,'new-fixture-password','new-fixture-password','4'*64,True,'x')
  self.assertEqual(len(self.auth.devices(first['user_id'])),1);self.assertIsNone(self.auth.subject(first['access_token']));self.assertEqual(self.auth.subject(recovered['access_token']),first['user_id'])
  with self.assertRaises(AccountError):self.auth.login('buyer@example.test','strong-fixture-password','1'*64,'x')
 def test_same_timestamp_refund_remains_revoked_and_naive_date_rejected(self):
  first=self.activate();self.webhook(self.order(status='refunded'),'same-refund','orders/updated');self.webhook(self.order(),'same-paid')
  self.assertIsNone(self.auth.subject(first['access_token']))
  with self.assertRaises(AccountError):self.webhook(self.order(updated='2026-10-15T12:00:00'),'naive')
 def test_production_http_activation_gate_and_malformed_webhook(self):
  server=Server(('127.0.0.1',0),self.store);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start();url='http://127.0.0.1:'+str(server.server_port)
  def call(path,body=None,token=None,extra=None):
   headers=dict(extra or {})
   if token:headers['Authorization']='Bearer '+token
   raw=json.dumps(body).encode() if body is not None else None
   try:
    with urllib.request.urlopen(urllib.request.Request(url+path,data=raw,headers=headers),timeout=10) as response:return response.status,json.loads(response.read())
   except urllib.error.HTTPError as error:return error.code,json.loads(error.read())
  try:
   self.assertEqual(call('/v1/auth/session')[0],401);self.assertEqual(call('/v1/auth/register',{})[0],403)
   self.webhook(self.order());self.assertEqual(call('/v1/auth/start',{'email':'buyer@example.test'})[0],200)
   status,result=call('/v1/auth/activate',dict(email='buyer@example.test',code=self.mail[-1][1],password='strong-fixture-password',password_confirmation='strong-fixture-password',device_id='1'*64));self.assertEqual(status,200)
   self.assertEqual(len(call('/v1/auth/session',token=result['access_token'])[1]['devices']),1)
   self.assertEqual(call('/v1/auth/login',[])[0],400)
   body=self.order(updated=None);raw=json.dumps(body).encode();signature=base64.b64encode(hmac.new(b'fixture-secret',raw,hashlib.sha256).digest()).decode()
   headers={'X-Shopify-Hmac-Sha256':signature,'X-Shopify-Shop-Domain':'fixture.myshopify.com','X-Shopify-Topic':'orders/updated','X-Shopify-Webhook-Id':'malformed'}
   self.assertEqual(call('/v1/shopify/webhook',body,extra=headers)[0],400)
   self.assertEqual(call('/v1/auth/devices/revoke',{'device_id':'1'*64},result['access_token'])[0],200)
   self.assertEqual(call('/v1/bundles',token=result['access_token'])[0],401)
  finally:server.shutdown();server.server_close()

if __name__=='__main__':unittest.main()
