"""Provision exactly the validated private pack. Credentials only come from env."""
import argparse,hashlib,json,os,pathlib,sys,tempfile
from designs import load_catalog,verify_asset,PACK,MAX_ASSET,object_key,validate_png
from remote import SupabaseBlobs,RemoteError

def sync_manifest(source,catalog_path):
    root=pathlib.Path(source)
    if root.is_symlink() or not root.is_dir():
        raise ValueError('invalid_asset_directory')
    root=root.resolve()
    path=pathlib.Path(catalog_path)
    if not path.is_file():
        raise ValueError('catalog_not_found')
    data=load_catalog(path)
    designs=data.get('designs')
    if not isinstance(designs,list) or len(designs)!=56:
        raise ValueError('invalid_design_catalog_count')
    prevalidated=[]
    for item in designs:
        filename=item.get('filename')
        if not filename or not isinstance(filename,str):
            raise ValueError('invalid_catalog_entry')
        asset_file=root/filename
        if asset_file.is_symlink() or not asset_file.is_file() or asset_file.resolve().parent!=root:
            raise ValueError(f'missing_asset_file: {filename}')
        if not 0<asset_file.stat().st_size<=MAX_ASSET:
            raise ValueError(f'invalid_asset_size: {filename}')
        content=asset_file.read_bytes()
        if len(content)==0 or len(content)>MAX_ASSET:
            raise ValueError(f'invalid_asset_size: {filename}')
        width,height=validate_png(content)
        sha=hashlib.sha256(content).hexdigest()
        prevalidated.append((item,sha,len(content),width,height))
    changed=False
    for item,sha,size,width,height in prevalidated:
        if (item.get('sha256')!=sha or item.get('bytes')!=size
                or item.get('width')!=width or item.get('height')!=height):
            item['sha256']=sha
            item['bytes']=size
            item['width']=width
            item['height']=height
            changed=True
    if changed:
        temporary=None
        try:
            with tempfile.NamedTemporaryFile(mode='w',encoding='utf-8',dir=path.parent,prefix='.design-catalog-',suffix='.tmp',delete=False) as stream:
                temporary=pathlib.Path(stream.name)
                stream.write(json.dumps(data,indent=2,ensure_ascii=False)+'\n')
                stream.flush();os.fsync(stream.fileno())
            load_catalog(temporary)
            os.replace(temporary,path)
        finally:
            if temporary is not None:temporary.unlink(missing_ok=True)
    return len(prevalidated),changed

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
    elif status==200:
        bucket=json.loads(body)
        if not isinstance(bucket,dict) or bucket.get('public') is not False:
            raise RemoteError('design_bucket_unavailable_or_public')
        limit=bucket.get('file_size_limit')
        if limit is not None and (type(limit) is not int or limit<max(len(data) for _,data in assets)):
            raise RemoteError('private_design_bucket_size_limit_too_small')
    else:raise RemoteError('design_bucket_unavailable')
    blobs.verify_bucket()
    for item,data in assets:
        key=object_key(item)
        url=blobs.base+'/object/wallet-designs/'+key
        status,_=blobs.request(url,'POST',dict(blobs.headers,**{'Content-Type':'image/png','x-upsert':'false'}),data,65536)
        if status not in (200,201,400,409):raise RemoteError('design_not_uploaded')
        status,received=blobs.request(blobs.base+'/object/authenticated/wallet-designs/'+key,'GET',blobs.headers,maximum=MAX_ASSET)
        if status!=200:raise RemoteError('design_readback_failed')
        verify_asset(item,received)
        if received!=data:raise RemoteError('design_readback_mismatch')
    return len(assets)

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--source',required=True)
    parser.add_argument('--catalog',default=str(pathlib.Path(__file__).with_name('design-catalog.json')))
    parser.add_argument('--only',nargs='+',metavar='DESIGN_ID',help='Validate and provision only these exact catalog IDs')
    parser.add_argument('--sync-manifest',action='store_true',help='Prevalidate all 56 assets and update sha256/bytes/dimensions in catalog')
    parser.add_argument('--manifest-only',action='store_true',help='Prevalidate all 56 assets and update catalog offline without uploading')
    args=parser.parse_args()
    if args.sync_manifest or args.manifest_only:
        try:
            count,updated=sync_manifest(args.source,args.catalog)
            print(f'Prevalidated {count} designs. Catalog manifest {"updated" if updated else "already in sync"}.')
        except Exception as error:
            print(f'Sync manifest failed: {error}',file=sys.stderr);sys.exit(1)
        if args.manifest_only:
            sys.exit(0)
    try:
        count=provision(os.environ,args.source,args.catalog,only_ids=args.only)
    except Exception:
        print('Private catalog provisioning failed; no credentials or asset paths logged.',file=sys.stderr);sys.exit(1)
    print(f'Private catalog verified: {count} designs.')
