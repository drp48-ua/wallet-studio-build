"""Windows-only Rust/WinHTTP/DPAPI integration using temporary synthetic data."""
import os,pathlib,subprocess,sys,tempfile,threading
project=pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0,str(project/'server'))
from app import Store,Server
scratch=project/'server'/'test-runtime';scratch.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=scratch,prefix='desktop-fixture-') as temp:
    server=Server(('127.0.0.1',0),Store(temp,True))
    threading.Thread(target=server.serve_forever,daemon=True).start()
    env=os.environ.copy()
    env['WALLET_STUDIO_TEST_FIXTURE']='1'
    env['WALLET_STUDIO_INTEGRATION_URL']='http://127.0.0.1:'+str(server.server_port)
    try:
        result=subprocess.run(['cargo','test','--release','--locked','--manifest-path',str(project/'windows-source'/'Cargo.toml'),'--bin','aircard','cloud_sync::tests::customer_cloud_server_e2e','--','--ignored','--exact','--test-threads=1'],env=env,cwd=project)
    finally:server.shutdown();server.server_close()
    assert pathlib.Path(temp).resolve().is_relative_to(scratch.resolve())
sys.exit(result.returncode)
