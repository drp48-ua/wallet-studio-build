"""Adapter tests use fakes, not a live PostgreSQL/Supabase/Brevo claim."""
import contextlib,hashlib,json,os,pathlib,sys,tempfile,unittest
from unittest.mock import patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
from remote import PgDatabase,SupabaseBlobs,RemoteError,sql_for_postgres
from app import Store
from accounts import Accounts,AccountError

class FakeConnection:
 def __init__(self):self.calls=[]
 def execute(self,sql,args=()):self.calls.append((sql,args));return self
 def fetchone(self):return None
 @contextlib.contextmanager
 def transaction(self):
  self.calls.append(('savepoint',()))
  try:yield
  except Exception:self.calls.append(('rollback_savepoint',()));raise
 def commit(self):self.calls.append(('commit',()))
 def rollback(self):self.calls.append(('rollback',()))
 def close(self):self.calls.append(('close',()))

class RemoteTests(unittest.TestCase):
 def setUp(self):
  environment=patch.dict(os.environ,{'WALLET_STORAGE_MODE':'local','RENDER':''});environment.start();self.addCleanup(environment.stop)
 def test_postgres_uses_one_transaction_lock_across_nested_account_operations(self):
  connections=[];options=[]
  def connector(url,**kwargs):options.append(kwargs);connection=FakeConnection();connections.append(connection);return connection
  database=PgDatabase('postgresql://synthetic-only',connector)
  with database.lock:
   with database.connect() as first:first.execute('SELECT id FROM users WHERE email=?',('fixture@example.test',))
   with database.connect() as second:self.assertIs(first.connection,second.connection)
  self.assertEqual(len(connections),1);self.assertEqual(options[0]['sslmode'],'verify-full');self.assertIsNone(options[0]['prepare_threshold'])
  self.assertEqual(sum('pg_advisory_xact_lock' in sql for sql,_ in connections[0].calls),1)
  self.assertIn(('commit',()),connections[0].calls);self.assertIn(('SELECT id FROM wallet_private.users WHERE email=%s',('fixture@example.test',)),connections[0].calls)
 def test_failed_otp_commits_prior_savepoint_but_database_error_rolls_back(self):
  for error,expected in ((AccountError('invalid_activation'),'commit'),(RuntimeError('failed'),'rollback')):
   connection=FakeConnection();database=PgDatabase('postgresql://fixture',lambda *args,**kw:connection)
   with self.assertRaises(type(error)):
    with database.lock:
     with database.connect() as db:db.execute('UPDATE activation SET attempts=attempts+1 WHERE email=?',('fixture@example.test',))
     raise error
   self.assertIn((expected,()),connection.calls)
 def test_postgres_dialect_upserts_and_private_schema(self):
  sql=sql_for_postgres('INSERT OR REPLACE INTO activation VALUES(?,?,?,?,?)')
  self.assertIn('wallet_private.activation',sql);self.assertIn('ON CONFLICT(email) DO UPDATE',sql);self.assertNotIn('?',sql)
  self.assertIn('DOUBLE PRECISION',sql_for_postgres('CREATE TABLE purchase_orders(updated REAL)'))
  self.assertIsNone(sql_for_postgres('BEGIN IMMEDIATE'))
  self.assertEqual(sql_for_postgres("SELECT 'users' FROM users"),"SELECT 'users' FROM wallet_private.users")
 def test_private_storage_create_only_and_authenticated_readback(self):
  data=b'synthetic immutable bundle';ident=hashlib.sha256(data).hexdigest();calls=[]
  def request(url,method,headers,body=None,maximum=1):
   calls.append((url,method,headers,body))
   if '/bucket/' in url:return 200,b'{"public":false}'
   return (409,b'') if method=='POST' else (200,data)
  blobs=SupabaseBlobs('https://fixture.supabase.co','secret-fixture','wallet-originals',request);blobs.verify_bucket();blobs.create('synthetic-user',ident,data)
  self.assertEqual(calls[1][2]['x-upsert'],'false');self.assertIn('/object/authenticated/wallet-originals/synthetic-user/',calls[2][0]);self.assertEqual(calls[2][2]['Authorization'],'Bearer secret-fixture')
  self.assertTrue(all('/public/' not in row[0] for row in calls))
 def test_public_bucket_tampering_and_cross_owner_path_rejected(self):
  blobs=SupabaseBlobs('https://fixture.supabase.co','fixture-key','wallet-originals',lambda *args,**kwargs:(200,b'{"public":true}'))
  with self.assertRaises(RemoteError):blobs.verify_bucket()
  with self.assertRaises(RemoteError):blobs.path('../other','0'*64)
  blobs.request=lambda *args,**kwargs:(200,b'corrupt')
  with self.assertRaises(RemoteError):blobs.read('synthetic-user','0'*64)
 def test_remote_missing_configuration_never_creates_ephemeral_vault(self):
  with tempfile.TemporaryDirectory() as temp:
   target=pathlib.Path(temp)/'must-not-be-created'
   with patch.dict(os.environ,{'WALLET_STORAGE_MODE':'remote'},clear=True):
    with self.assertRaises(ValueError):Store(target)
   with patch.dict(os.environ,{'RENDER':'true','WALLET_STORAGE_MODE':'local'},clear=True):
    with self.assertRaises(ValueError):Store(target)
   self.assertFalse(target.exists())
 def test_brevo_uses_https_api_not_smtp_and_failure_is_sanitized(self):
  with tempfile.TemporaryDirectory() as temp:
   store=Store(temp);auth=Accounts(store,{'BREVO_API_KEY':'secret-fixture-key','BREVO_SENDER_EMAIL':'sender@example.test'})
   with patch('accounts.https_request',return_value=(201,b'{"messageId":"synthetic"}')) as request:
    auth.send_mail('buyer@example.test','123456');args=request.call_args.args;self.assertEqual(args[0],'https://api.brevo.com/v3/smtp/email');self.assertEqual(args[2]['api-key'],'secret-fixture-key');self.assertIn('123456',json.loads(args[3])['textContent'])
   with patch('accounts.https_request',return_value=(403,b'private-response')):
    with self.assertRaisesRegex(AccountError,'^email_delivery_unavailable$'):auth.send_mail('buyer@example.test','123456')

if __name__=='__main__':unittest.main()
