"""Safe startup diagnostics with synthetic failures only; no real provider calls."""
import contextlib,io,os,pathlib,ssl,sys,threading,unittest,urllib.error
from unittest.mock import Mock,patch
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
import app
from remote import PgDatabase,RemoteError,StartupError,SupabaseBlobs,safe_failure,startup_stage

PRIVATE='postgresql://private-user:private-password@192.0.2.17/private-db secret-key private-response /private/path'
ENV={'WALLET_STORAGE_MODE':'remote','DATABASE_URL':PRIVATE,'SUPABASE_URL':'https://fixture.supabase.co','SUPABASE_SERVICE_ROLE_KEY':'synthetic-secret','SUPABASE_STORAGE_BUCKET':'wallet-originals'}

class DatabaseFailure(Exception):
    def __init__(self,state):super().__init__(PRIVATE);self.sqlstate=state

class StartupTests(unittest.TestCase):
    def test_database_connect_sqlstate_and_tls_are_safe(self):
        for error,reason,state in ((DatabaseFailure('28P01'),'database_authentication','28P01'),(DatabaseFailure('08001'),'database_connection','08001'),(RuntimeError(PRIVATE),'database_connection',None),(ssl.SSLCertVerificationError(PRIVATE),'tls_verification',None),(ImportError(PRIVATE),'dependency_missing',None)):
            with self.subTest(reason=reason):
                with self.assertRaises(StartupError) as caught:
                    PgDatabase(PRIVATE,Mock(side_effect=error)).open()
                line=caught.exception.log_line()
                self.assertIn('stage=database_connect',line);self.assertIn('reason='+reason,line)
                self.assertEqual(caught.exception.diagnostic['sqlstate'],state)
                self.assertNotIn('private',line);self.assertNotIn('192.0.2.17',line)

    def test_store_distinguishes_every_remote_stage(self):
        for stage,target in (('storage_configuration','storage_constructor'),('database_schema','initialize'),('storage_bucket','verify_bucket'),('account_schema','accounts'),('database_permissions','secure_tables')):
            with self.subTest(stage=stage):
                database=Mock();database.lock=threading.RLock();blobs=Mock();failure=DatabaseFailure('42501')
                if target in ('initialize','secure_tables'):getattr(database,target).side_effect=failure
                if target=='verify_bucket':blobs.verify_bucket.side_effect=failure
                with patch.dict(os.environ,ENV,clear=True),patch('app.PgDatabase',return_value=database),patch('app.SupabaseBlobs',side_effect=failure if target=='storage_constructor' else None,return_value=blobs),patch('app.Accounts',side_effect=failure if target=='accounts' else None):
                    with self.assertRaises(StartupError) as caught:app.Store('/unused-synthetic')
                self.assertEqual(caught.exception.stage,stage)
                self.assertIn('sqlstate=42501',caught.exception.log_line())
                self.assertNotIn(PRIVATE,caught.exception.log_line())
        with patch.dict(os.environ,{'WALLET_STORAGE_MODE':'remote'},clear=True):
            with self.assertRaises(StartupError) as caught:app.Store('/unused-synthetic')
        self.assertEqual(caught.exception.stage,'configuration')

    def test_bucket_http_response_json_and_private_checks(self):
        for response,reason,status in (((401,PRIVATE.encode()),'storage_http',401),((503,PRIVATE.encode()),'storage_http',503),((200,PRIVATE.encode()),'storage_invalid_response',None),((200,b'[]'),'storage_invalid_response',None),((200,b'{"public":true}'),'storage_not_private',None)):
            with self.subTest(reason=reason,status=status):
                blobs=SupabaseBlobs('https://fixture.supabase.co','synthetic-secret','wallet-originals',Mock(return_value=response))
                with self.assertRaises(StartupError) as caught:
                    with startup_stage('storage_bucket'):blobs.verify_bucket()
                diagnostic=caught.exception.diagnostic
                self.assertEqual(diagnostic['reason'],reason);self.assertEqual(diagnostic['http_status'],status)
                self.assertNotIn(PRIVATE,caught.exception.log_line())
        request=Mock(side_effect=[(404,b''),(403,PRIVATE.encode())])
        with self.assertRaises(RemoteError) as caught:SupabaseBlobs('https://fixture.supabase.co','synthetic-secret','wallet-originals',request).verify_bucket()
        self.assertEqual(safe_failure(caught.exception)['http_status'],403)

    def test_untrusted_exception_metadata_never_becomes_a_log_message(self):
        error=RemoteError(PRIVATE,reason=PRIVATE,sqlstate=PRIVATE,http_status=PRIVATE)
        self.assertEqual(StartupError(PRIVATE,error).log_line(),'startup_failed stage=configuration reason=configuration_invalid')
        error.reason=['secret']
        self.assertEqual(safe_failure(error)['reason'],'configuration_invalid')
        error=urllib.error.URLError(ssl.SSLCertVerificationError(PRIVATE))
        self.assertEqual(safe_failure(error)['reason'],'tls_verification')
        with self.assertRaises(StartupError) as caught:
            with startup_stage('database_schema'):raise DatabaseFailure('3F000')
        self.assertEqual(caught.exception.log_line(),'startup_failed stage=database_schema reason=database_schema sqlstate=3F000')

    def test_main_emits_only_sanitized_stage_and_generic_message(self):
        output=io.StringIO()
        failure=StartupError('database_connect',DatabaseFailure('28P01'))
        with patch.object(sys,'argv',['wallet-server']),patch('app.Store',side_effect=failure),contextlib.redirect_stderr(output):
            with self.assertRaises(SystemExit) as caught:app.main()
        self.assertEqual(caught.exception.code,2)
        self.assertIn('startup_failed stage=database_connect reason=database_authentication sqlstate=28P01',output.getvalue())
        self.assertNotIn(PRIVATE,output.getvalue());self.assertNotIn('Traceback',output.getvalue())

if __name__=='__main__':unittest.main()
