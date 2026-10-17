"""Provision exactly the validated private pack. Credentials only come from env."""
import argparse,json,os,pathlib,sys
from designs import load_catalog,verify_asset,PACK,MAX_ASSET
from remote import SupabaseBlobs,RemoteError

def provision(config,source,catalog_path,request=None,only_ids=None):
    catalog=load_catalog(catalog_path);root=pathlib.Path(source)
    items=catalog['designs']
    if only_ids is not None:
        if not isinstance(only_ids,(list,tuple)) or not only_ids or any(not isinstance(ident,str) for ident in only_ids) or len(set(only_ids))!=len(only_ids):raise ValueError('invalid_design_selection')
        selected=set(only_ids)
        if not selected.issubset({item['id'] for item in items}):raise ValueError('unknown_design_selection')
        items=[item for item in items if item['id'] in selected]
    if root.is_symlink() or not root.is_dir():raise ValueError('invalid_asset_directory')
    root=root.resolve();assets=[]
    for item in items:
        path=root/item['filename']
        if path.is_symlink() or not path.is_file() or path.resolve().parent!=root or path.stat().st_size>MAX_ASSET:raise ValueError('invalid_asset_file')
        data=path.read_bytes();verify_asset(item,data);assets.append((item,data))
    args={} if request is None else {'request':request}
    blobs=SupabaseBlobs(config['SUPABASE_URL'],config['SUPABASE_SERVICE_ROLE_KEY'],'wallet-designs',**args)
    status,body=blobs.request(blobs.base+'/bucket/wallet-designs','GET',blobs.headers,maximum=65536)
    if status==404:
        payload=json.dumps({'id':'wallet-designs','name':'wallet-designs','public':False,'file_size_limit':MAX_ASSET,'allowed_mime_types':['image/png']}).encode()
        status,_=blobs.request(blobs.base+'/bucket','POST',dict(blobs.headers,**{'Content-Type':'application/json'}),payload,65536)
        if status not in (200,201):raise RemoteError('private_design_bucket_not_created')
    blobs.verify_bucket()
    for item,data in assets:
        url=blobs.base+'/object/wallet-designs/'+PACK+'/'+item['filename']
        status,_=blobs.request(url,'POST',dict(blobs.headers,**{'Content-Type':'image/png','x-upsert':'false'}),data,65536)
        if status not in (200,201,400,409):raise RemoteError('design_not_uploaded')
        status,received=blobs.request(blobs.base+'/object/authenticated/wallet-designs/'+PACK+'/'+item['filename'],'GET',blobs.headers,maximum=MAX_ASSET)
        if status!=200:raise RemoteError('design_readback_failed')
        verify_asset(item,received)
        if received!=data:raise RemoteError('design_readback_mismatch')
    return len(assets)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--source',required=True);parser.add_argument('--catalog',default=str(pathlib.Path(__file__).with_name('design-catalog.json')));parser.add_argument('--only',nargs='+',metavar='DESIGN_ID',help='Validate and provision only these exact catalog IDs');args=parser.parse_args()
    try:
        count=provision(os.environ,args.source,args.catalog,only_ids=args.only)
    except Exception:
        print('Private catalog provisioning failed; no credentials or asset paths logged.',file=sys.stderr);sys.exit(1)
    print(f'Private catalog verified: {count} designs.')
