"""Windows-only Rust/WinHTTP/DPAPI integration using temporary synthetic data."""
import os,pathlib,subprocess,sys,tempfile,threading
project=pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0,str(project/'server'))
from app import Store,Server
scratch=project/'server'/'test-runtime';scratch.mkdir(exist_ok=True)
if os.name=='nt' and not str(scratch).startswith('\\\\?\\'):
    # Blobs contain both a user directory and a full SHA-256 filename. Keep
    # creation and cleanup on the same long-path API in deep Windows checkouts.
    scratch=pathlib.Path('\\\\?\\'+str(scratch.resolve()))
with tempfile.TemporaryDirectory(dir=scratch,prefix='desktop-fixture-') as temp:
    server=Server(('127.0.0.1',0),Store(temp,True))
    # Join request handlers before TemporaryDirectory removes the SQLite/blobs.
    # The production daemon setting otherwise races Windows directory cleanup.
    server.daemon_threads=False
    threading.Thread(target=server.serve_forever,daemon=True).start()
    env=os.environ.copy()
    env['WALLET_STUDIO_TEST_FIXTURE']='1'
    env['WALLET_STUDIO_INTEGRATION_URL']='http://127.0.0.1:'+str(server.server_port)
    try:
        result=subprocess.run(['cargo','test','--release','--locked','--manifest-path',str(project/'windows-source'/'Cargo.toml'),'--bin','aircard','cloud_sync::tests::customer_cloud_server_e2e','--','--ignored','--exact','--test-threads=1'],env=env,cwd=project)
    finally:server.shutdown();server.server_close()
    assert pathlib.Path(temp).resolve().is_relative_to(scratch.resolve())
sys.exit(result.returncode)
