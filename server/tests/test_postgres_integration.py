"""Opt-in real PostgreSQL test. Never uses production DATABASE_URL implicitly.

Requires a dedicated synthetic database, installed requirements, and explicit
WALLET_TEST_POSTGRES_CONFIRM=isolated-synthetic-database. Skipped otherwise.
"""
import base64,concurrent.futures,hashlib,hmac,json,os,pathlib,secrets,sys,unittest
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from app import Store
from accounts import Accounts,AccountError
from remote import PgDatabase

ENABLED=bool(os.environ.get('WALLET_TEST_POSTGRES_URL')) and os.environ.get('WALLET_TEST_POSTGRES_CONFIRM')=='isolated-synthetic-database'

@unittest.skipUnless(ENABLED,'dedicated PostgreSQL fixture and explicit confirmation required')
class PostgreSQLIntegration(unittest.TestCase):
 def store(self):
  store=object.__new__(Store);store.remote=True;store.remote_db=PgDatabase(os.environ['WALLET_TEST_POSTGRES_URL']);store.lock=store.remote_db.lock;store.limits={};store.allow_registration=False;store.session_seconds=3600
  store.remote_db.initialize();store.accounts=Accounts(store,{'WALLET_AUTH_SECRET':'synthetic-secret-'*3,'SHOPIFY_SHOP_DOMAIN':'fixture.myshopify.com','SHOPIFY_PRODUCT_IDS':'123','SHOPIFY_WEBHOOK_SECRET':'fixture-key'},lambda email,code:self.mail.append(code));store.remote_db.secure_tables();return store
 def test_independent_store_connections_share_three_device_limit_and_otp_consumption(self):
  self.mail=[];a=self.store();b=self.store();email='synthetic-pg-'+secrets.token_hex(12)+'@example.test';order=str(secrets.randbelow(10**28));user=None
  body=json.dumps({'id':order,'email':email,'updated_at':'2026-10-15T12:00:00Z','financial_status':'paid','line_items':[{'id':1,'product_id':123,'quantity':1}]}).encode()
  signature=base64.b64encode(hmac.new(b'fixture-key',body,hashlib.sha256).digest()).decode()
  try:
   a.accounts.webhook(body,signature,'fixture.myshopify.com','orders/paid','fixture-'+order);a.accounts.start(email);code=self.mail[-1]
   first=a.accounts.activate(email,code,'synthetic-password-strong','synthetic-password-strong','1'*64,'synthetic-peer');user=first['user_id']
   with self.assertRaises(AccountError):b.accounts.activate(email,code,'synthetic-password-strong','synthetic-password-strong','2'*64,'synthetic-peer')
   def login(index):
    try:return (a if index%2 else b).accounts.login(email,'synthetic-password-strong',f'{index:064x}','synthetic-peer')
    except AccountError:return None
   with concurrent.futures.ThreadPoolExecutor(5) as pool:results=list(pool.map(login,range(2,7)))
   self.assertEqual(sum(item is not None for item in results),2);self.assertEqual(len(b.accounts.devices(user)),3)
   with a.connect() as db:self.assertEqual(db.execute("SELECT relrowsecurity FROM pg_class JOIN pg_namespace n ON n.oid=relnamespace WHERE n.nspname='wallet_private' AND relname='users'").fetchone()[0],True)
  finally:
   with a.connect() as db:
    if user:
     for table in ('sessions','computers','bundles'):db.execute(f'DELETE FROM {table} WHERE user_id=?',(user,))
     db.execute('DELETE FROM users WHERE id=?',(user,))
    db.execute('DELETE FROM activation WHERE email=?',(email,));db.execute('DELETE FROM purchase_orders WHERE id=?',(order,));db.execute('DELETE FROM webhook_events WHERE id=?',('fixture-'+order,))

if __name__=='__main__':unittest.main()
