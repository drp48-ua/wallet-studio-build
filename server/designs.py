"""Private paid catalog. No signed/public URLs, client product claims or asset paths."""
import hashlib,json,pathlib,re,struct,zlib
from remote import SupabaseBlobs,RemoteError
PACK='studio-50-v1'
MAX_ASSET=1024*1024
MAX_DESIGNS=100
EXPANDED_CATALOG_HEADER='X-Wallet-Studio-Catalog'
EXPANDED_CATALOG_CAPABILITY='expanded-v1'

def validate_png(data):
    if not 0<len(data)<=MAX_ASSET or not data.startswith(b'\x89PNG\r\n\x1a\n'):raise ValueError('invalid_design_png')
    pos=8;compressed=bytearray();header=None;ended=False
    while pos<len(data):
        if pos+12>len(data):raise ValueError('invalid_design_png')
        size=struct.unpack('>I',data[pos:pos+4])[0];kind=data[pos+4:pos+8];chunk=data[pos+8:pos+8+size]
        if pos+12+size>len(data) or zlib.crc32(kind+chunk)!=struct.unpack('>I',data[pos+8+size:pos+12+size])[0]:raise ValueError('invalid_design_png')
        if header is None:
            if kind!=b'IHDR' or size!=13:raise ValueError('invalid_design_png')
            header=struct.unpack('>IIBBBBB',chunk)
            if header[:2]!=(320,200) or header[2]!=8 or header[3] not in (2,6) or header[4:]!=(0,0,0):raise ValueError('invalid_design_dimensions')
        elif kind==b'IDAT':compressed.extend(chunk)
        elif kind==b'IEND':
            if size:raise ValueError('invalid_design_png')
            ended=True;pos+=12;break
        elif kind in (b'IHDR',b'acTL',b'fcTL',b'fdAT') or (kind[0]&32)==0:raise ValueError('unsupported_design_png')
        pos+=size+12
    if not ended or pos!=len(data):raise ValueError('invalid_design_png')
    stride=320*(3 if header[3]==2 else 4)+1;expected=200*stride
    decoder=zlib.decompressobj()
    try:pixels=decoder.decompress(compressed,expected+1)
    except zlib.error:raise ValueError('invalid_design_pixels') from None
    if len(pixels)!=expected or not decoder.eof or decoder.unused_data or decoder.unconsumed_tail or any(pixels[i]>4 for i in range(0,expected,stride)):raise ValueError('invalid_design_pixels')

def load_catalog(path):
    path=pathlib.Path(path)
    if not path.is_file() or path.stat().st_size>65536:raise ValueError('design_catalog_unconfigured')
    value=json.loads(path.read_bytes())
    if not isinstance(value,dict) or type(value.get('schema')) is not int or value.get('schema')!=1 or value.get('pack_id')!=PACK or not isinstance(value.get('designs'),list) or not 50<=len(value['designs'])<=MAX_DESIGNS:raise ValueError('invalid_design_catalog')
    for index,item in enumerate(value['designs'],1):
        ident=f'ws-{index:03}'
        if not isinstance(item,dict) or item.get('id')!=ident or item.get('filename')!=ident+'.png' or not isinstance(item.get('title'),str) or not 1<=len(item['title'])<=80 or any(ord(c)<32 for c in item['title']) or not re.fullmatch('[0-9a-f]{64}',item.get('sha256','')) or type(item.get('bytes')) is not int or not 0<item['bytes']<=MAX_ASSET or (item.get('width'),item.get('height'))!=(320,200):raise ValueError('invalid_design_catalog')
    return value

def catalog_for_client(catalog,capability):
    # Versions 0.6.5–0.6.7 reject any catalogue whose count differs from 50.
    designs=catalog['designs'] if capability==EXPANDED_CATALOG_CAPABILITY else catalog['designs'][:50]
    return dict(catalog,title=f'{len(designs)} diseños',designs=designs,entitled=True)

def verify_asset(item,data):
    if len(data)!=item['bytes'] or hashlib.sha256(data).hexdigest()!=item['sha256']:raise ValueError('design_integrity_error')
    validate_png(data)

class DesignCatalog:
    def __init__(self,config):
        self.config=config
    def catalog(self):return load_catalog(self.config.get('WALLET_DESIGN_CATALOG',str(pathlib.Path(__file__).with_name('design-catalog.json'))))
    def blobs(self):return SupabaseBlobs(self.config['SUPABASE_URL'],self.config['SUPABASE_SERVICE_ROLE_KEY'],'wallet-designs')
    def read(self,item):
        blobs=self.blobs()
        status,body=blobs.request(blobs.base+'/bucket/wallet-designs','GET',blobs.headers,maximum=65536)
        if status!=200:raise RemoteError('design_bucket_unavailable_or_public')
        metadata=json.loads(body)
        if not isinstance(metadata,dict) or metadata.get('public') is not False:raise RemoteError('design_bucket_unavailable_or_public')
        status,data=blobs.request(blobs.base+'/object/authenticated/wallet-designs/'+PACK+'/'+item['filename'],'GET',blobs.headers,maximum=MAX_ASSET)
        if status!=200:raise RemoteError('design_unavailable')
        verify_asset(item,data);return data
