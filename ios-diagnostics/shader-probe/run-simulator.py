import json,pathlib,subprocess,time,sys,re

def call(*args):
    return subprocess.check_output(['xcrun','simctl',*args],text=True).strip()

devices=json.loads(call('list','devices','available','-j'))['devices']
def runtime_version(identifier):
    match=re.search(r'iOS-(\d+(?:-\d+)*)$',identifier)
    return tuple(map(int,match[1].split('-'))) if match else ()
runtime_choices=[(runtime,group) for runtime,group in devices.items() if runtime_version(runtime) and any(d['name'].startswith('iPhone') for d in group)]
runtime_choices.sort(key=lambda item: runtime_version(item[0]),reverse=True)
choices=[d for d in runtime_choices[0][1] if d['name'].startswith('iPhone')] if runtime_choices else []
if not choices:
    raise RuntimeError('No available iPhone simulator')
device=choices[0]['udid']
booted=choices[0]['state']=='Booted'
try:
    if not booted: call('boot',device)
    call('bootstatus',device,'-b')
    call('install',device,str(pathlib.Path('build/iphonesimulator/ShaderDiagnostics.app').resolve()))
    call('launch',device,'local.walletstudio.shaderdiagnostics')
    container=pathlib.Path(call('get_app_container',device,'local.walletstudio.shaderdiagnostics','data'))
    report=container/'Documents/shader-report.json'
    deadline=time.monotonic()+30
    while not report.exists() and time.monotonic()<deadline: time.sleep(.5)
    sys.path.insert(0,str(pathlib.Path('tests').resolve()))
    from validate_report import validate
    data=json.loads(report.read_text())
    validate(data)
    pathlib.Path('build/shader-report-simulator.json').write_text(json.dumps(data,indent=2))
    print(json.dumps(data,indent=2))
finally:
    if not booted: call('shutdown',device)
