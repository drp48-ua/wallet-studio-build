"""Private paid catalog. No signed/public URLs, client product claims or asset paths."""
import collections,hashlib,json,pathlib,re,struct,threading,zlib
from remote import SupabaseBlobs,RemoteError,configured_blobs
PACK='studio-50-v1'
MAX_ASSET=4*1024*1024
LEGACY_ASSET=1024*1024
LEGACY_SIZE=(320,200)
NATIVE_SIZE=(1536,969)
ASSET_LIMITS={LEGACY_SIZE:LEGACY_ASSET,NATIVE_SIZE:MAX_ASSET}
MAX_DESIGNS=100
DESIGN_CACHE_BYTES=64*1024*1024
DESIGN_CACHE_ENTRIES=128
LEGACY_COUNT=50
EXPANDED_CATALOG_HEADER='X-Wallet-Studio-Catalog'
EXPANDED_CATALOG_CAPABILITY='expanded-v1'
NATIVE_2K_CAPABILITY='native-2k-v1'
NATIVE_VARIANT_CAPABILITY='native-2k-v2'
# Released v1 clients sent this header only when listing, not downloading.
# Keep their metadata legacy; v2 negotiates originals on both requests.
NATIVE_CAPABILITIES=(NATIVE_VARIANT_CAPABILITY,)

DEFAULT_CATALOG_PATH=pathlib.Path(__file__).with_name('design-catalog.json')
DEFAULT_LEGACY_CATALOG_PATH=pathlib.Path(__file__).with_name('design-catalog.legacy.json')

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
            limit=ASSET_LIMITS.get(header[:2])
            if limit is None or len(data)>limit or header[2]!=8 or header[3] not in (2,6) or header[4:]!=(0,0,0):raise ValueError('invalid_design_dimensions')
        elif kind==b'IDAT':compressed.extend(chunk)
        elif kind==b'IEND':
            if size:raise ValueError('invalid_design_png')
            ended=True;pos+=12;break
        elif kind in (b'IHDR',b'acTL',b'fcTL',b'fdAT') or (kind[0]&32)==0:raise ValueError('unsupported_design_png')
        pos+=size+12
    if not ended or pos!=len(data):raise ValueError('invalid_design_png')
    width,height=header[:2];stride=width*(3 if header[3]==2 else 4)+1;expected=height*stride
    decoder=zlib.decompressobj()
    try:pixels=decoder.decompress(compressed,expected+1)
    except zlib.error:raise ValueError('invalid_design_pixels') from None
    if len(pixels)!=expected or not decoder.eof or decoder.unused_data or decoder.unconsumed_tail or any(pixels[i]>4 for i in range(0,expected,stride)):raise ValueError('invalid_design_pixels')
    return width,height

def load_catalog(path):
    path=pathlib.Path(path)
    if not path.is_file() or path.stat().st_size>65536:raise ValueError('design_catalog_unconfigured')
    value=json.loads(path.read_bytes())
    if not isinstance(value,dict) or type(value.get('schema')) is not int or value.get('schema')!=1 or value.get('pack_id')!=PACK or not isinstance(value.get('designs'),list) or not 50<=len(value['designs'])<=MAX_DESIGNS:raise ValueError('invalid_design_catalog')
    for index,item in enumerate(value['designs'],1):
        ident=f'ws-{index:03}'
        if not isinstance(item,dict) or item.get('id')!=ident or item.get('filename')!=ident+'.png' or not isinstance(item.get('title'),str) or not 1<=len(item['title'])<=80 or any(ord(c)<32 for c in item['title']) or not re.fullmatch('[0-9a-f]{64}',item.get('sha256','')):raise ValueError('invalid_design_catalog')
        limit=ASSET_LIMITS.get((item.get('width'),item.get('height')))
        if limit is None or type(item.get('bytes')) is not int or not 0<item['bytes']<=limit:raise ValueError('invalid_design_catalog')
    return value

def object_key(item,capability=None):
    if (item.get('width'),item.get('height'))==NATIVE_SIZE:
        return f'{PACK}/native/{item["id"]}-{item["sha256"]}.png'
    return f'{PACK}/{item["filename"]}'

def load_legacy_catalog(path):
    catalog=load_catalog(path)
    for item in catalog['designs']:
        if (item.get('width'),item.get('height'))!=LEGACY_SIZE or item.get('bytes',0)>LEGACY_ASSET:
            raise ValueError('invalid_legacy_catalog')
    return catalog

def legacy_designs(designs):
    # 0.6.5–0.6.8 clients only parse 320x200/1MiB entries and require contiguous ids from ws-001.
    for count,item in enumerate(designs):
        if (item.get('width'),item.get('height'))!=LEGACY_SIZE or item.get('bytes',0)>LEGACY_ASSET:return designs[:count]
    return designs

def catalog_for_client(catalog,capability):
    # Versions 0.6.5–0.6.7 reject any catalogue whose count differs from 50.
    designs=catalog['designs']
    # 0.6.10 sends v2 and accepts original-only metadata without a thumbnail.
    # Do not treat it as an unknown client and silently return 50 legacy entries.
    if capability in NATIVE_CAPABILITIES:served=designs
    elif capability in (EXPANDED_CATALOG_CAPABILITY,NATIVE_2K_CAPABILITY):served=legacy_designs(designs)
    else:served=legacy_designs(designs)[:LEGACY_COUNT]
    return dict(catalog,title=f'{len(served)} diseños',designs=served,entitled=True)

def verify_asset(item,data):
    limit=ASSET_LIMITS.get((item.get('width'),item.get('height')))
    if limit is None or len(data)>limit:raise ValueError('invalid_design_dimensions')
    if len(data)!=item.get('bytes') or hashlib.sha256(data).hexdigest()!=item.get('sha256'):raise ValueError('design_integrity_error')
    width,height=validate_png(data)
    if (width,height)!=(item.get('width'),item.get('height')):raise ValueError('invalid_design_dimensions')

class DesignCatalog:
    def __init__(self,config,*,cache_bytes=DESIGN_CACHE_BYTES,cache_entries=DESIGN_CACHE_ENTRIES):
        if type(cache_bytes) is not int or not 0<=cache_bytes<=DESIGN_CACHE_BYTES or type(cache_entries) is not int or not 0<=cache_entries<=DESIGN_CACHE_ENTRIES:raise ValueError('invalid_design_cache_limit')
        self.config=config
        # Process-local, ephemeral PNGs only. Authorization/pack checks remain
        # in the existing HTTP route before every call, including cache hits.
        self._cache=collections.OrderedDict();self._cache_size=0
        self._cache_bytes=cache_bytes;self._cache_entries=cache_entries
        # One bounded lock also deduplicates simultaneous cold reads without
        # an unbounded table of per-key locks or in-flight payload copies.
        self._cache_lock=threading.Lock()
    def native_catalog_path(self):
        return pathlib.Path(self.config.get('WALLET_DESIGN_CATALOG',str(DEFAULT_CATALOG_PATH)))
    def legacy_catalog_path(self):
        return pathlib.Path(self.config.get('WALLET_DESIGN_LEGACY_CATALOG',str(DEFAULT_LEGACY_CATALOG_PATH)))
    def catalog(self,capability=None):
        if capability in NATIVE_CAPABILITIES:
            return load_catalog(self.native_catalog_path())
        legacy_path=self.legacy_catalog_path()
        if legacy_path.is_file():
            return load_legacy_catalog(legacy_path)
        return load_legacy_catalog(self.native_catalog_path())
    def blobs(self):return configured_blobs(self.config,designs=True)
    def read(self,item,capability=None):
        origin=(self.config.get('WALLET_OBJECT_STORAGE','supabase'),self.config.get('SUPABASE_URL',''),self.config.get('R2_ACCOUNT_ID',''),self.config.get('R2_DESIGN_BUCKET',''))
        key=(origin,object_key(item,capability),item['sha256'],item['bytes'],item['width'],item['height'])
        with self._cache_lock:
            data=self._cache.get(key)
            if data is not None:
                try:verify_asset(item,data)
                except ValueError:
                    self._cache_size-=len(self._cache.pop(key))
                else:
                    self._cache.move_to_end(key);return data
            data=self._read_verified(item,capability)
            if len(data)<=self._cache_bytes and self._cache_entries:
                while self._cache and (self._cache_size+len(data)>self._cache_bytes or len(self._cache)>=self._cache_entries):
                    _,previous=self._cache.popitem(last=False);self._cache_size-=len(previous)
                self._cache[key]=data;self._cache_size+=len(data)
            return data
    def _read_verified(self,item,capability):
        blobs=self.blobs()
        if self.config.get('WALLET_OBJECT_STORAGE','supabase')=='r2':
            blobs.verify_bucket();data=blobs.read_key(object_key(item,capability),MAX_ASSET)
            verify_asset(item,data);return data
        status,body=blobs.request(blobs.base+'/bucket/wallet-designs','GET',blobs.headers,maximum=65536)
        if status!=200:raise RemoteError('design_bucket_unavailable_or_public')
        metadata=json.loads(body)
        if not isinstance(metadata,dict) or metadata.get('public') is not False:raise RemoteError('design_bucket_unavailable_or_public')
        key=object_key(item,capability)
        status,data=blobs.request(blobs.base+'/object/authenticated/wallet-designs/'+key,'GET',blobs.headers,maximum=MAX_ASSET)
        if status!=200:raise RemoteError('design_unavailable')
        verify_asset(item,data);return data
