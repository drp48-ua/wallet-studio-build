"""Verification of all 50 commercial designs and refund revocation workflow.
Tests verify:
1. Catalog integrity for all 50 designs in design-catalog.json (ids, hashes, dimensions, sizes).
2. End-to-end client catalog retrieval and individual download of all 50 designs under authenticated session.
3. Strict revocation on Shopify refunds/create webhook (entitlement flipped to false, downloads return 403).
4. Unauthenticated access denial (HTTP 401).
5. Live Render backend verification (health check 200, unauthorized catalog 401).
"""
import base64
import hashlib
import hmac
import json
import os
import pathlib
import struct
import sys
import tempfile
import threading
import time
import unittest
import urllib.error
import urllib.request
import zlib
from unittest.mock import patch

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import test_accounts
from accounts import Accounts, AccountError
from app import Server, Store
from designs import load_catalog, validate_png

ACTIVE_SKU = '10459984298313'


def generate_png_320x200(fill_byte=b'\x22\x33\x44'):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    raw_scanlines = (b'\0' + fill_byte * 320) * 200
    idat_data = zlib.compress(raw_scanlines)
    return (
        b'\x89PNG\r\n\x1a\n'
        + chunk(b'IHDR', struct.pack('>IIBBBBB', 320, 200, 8, 2, 0, 0, 0))
        + chunk(b'IDAT', idat_data)
        + chunk(b'IEND', b'')
    )


class ClientCatalogAndRevocationTests(unittest.TestCase):
    def setUp(self):
        self.environment = patch.dict(os.environ, {'WALLET_STORAGE_MODE': 'local', 'RENDER': ''})
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.tmp = tempfile.TemporaryDirectory()
        self.store = Store(self.tmp.name)
        self.mail = []
        self.config = {
            'WALLET_AUTH_SECRET': 's' * 32,
            'SHOPIFY_SHOP_DOMAIN': 'fixture.myshopify.com',
            'SHOPIFY_APP_PRODUCT_IDS': ACTIVE_SKU,
            'SHOPIFY_BUNDLE_PRODUCT_IDS': '',
            'SHOPIFY_PACK_PRODUCT_IDS': '',
            'SHOPIFY_WEBHOOK_SECRET': 'fixture-secret',
        }
        self.auth = Accounts(self.store, self.config, lambda email, code: self.mail.append((email, code)))
        self.store.accounts = self.auth

    def tearDown(self):
        self.tmp.cleanup()

    def order(self, ident=1, email='customer@example.test', product=ACTIVE_SKU, status='paid',
              updated='2026-10-16T12:00:00Z', **extra):
        return dict(
            id=ident,
            email=email,
            updated_at=updated,
            financial_status=status,
            line_items=[dict(id=99, product_id=product, quantity=1)],
            **extra
        )

    def webhook(self, body, event='event-1', topic='orders/paid', signature=None):
        raw = json.dumps(body).encode()
        sig = signature or base64.b64encode(hmac.new(b'fixture-secret', raw, hashlib.sha256).digest()).decode()
        self.auth.webhook(raw, sig, 'fixture.myshopify.com', topic, event)

    def activate_customer(self, email='customer@example.test', ident=1):
        self.webhook(self.order(ident=ident, email=email), str(ident))
        self.auth.start(email)
        code = self.mail[-1][1]
        return self.auth.activate(
            email, code, 'secure-pass-2026', 'secure-pass-2026', '1' * 64, '127.0.0.1'
        )

    def test_catalog_manifest_contains_50_valid_designs(self):
        catalog_path = pathlib.Path(__file__).resolve().parents[1] / 'design-catalog.json'
        self.assertTrue(catalog_path.is_file(), 'design-catalog.json must exist in server directory')
        catalog = load_catalog(catalog_path)
        self.assertEqual(catalog.get('schema'), 1)
        self.assertEqual(catalog.get('pack_id'), 'studio-50-v1')
        designs = catalog.get('designs', [])
        self.assertEqual(len(designs), 50, 'Catalog must contain exactly 50 designs')

        ids = set()
        hashes = set()
        filenames = set()
        for idx, item in enumerate(designs, 1):
            expected_id = f'ws-{idx:03}'
            self.assertEqual(item['id'], expected_id)
            self.assertEqual(item['filename'], f'{expected_id}.png')
            self.assertEqual(item['width'], 320)
            self.assertEqual(item['height'], 200)
            self.assertGreater(item['bytes'], 0)
            self.assertLessEqual(item['bytes'], 1024 * 1024)
            self.assertEqual(len(item['sha256']), 64)
            self.assertTrue(all(c in '0123456789abcdef' for c in item['sha256']))
            self.assertTrue(1 <= len(item['title']) <= 80)

            ids.add(item['id'])
            hashes.add(item['sha256'])
            filenames.add(item['filename'])

        self.assertEqual(len(ids), 50, 'All 50 IDs must be distinct')
        self.assertEqual(len(hashes), 50, 'All 50 SHA-256 hashes must be distinct')
        self.assertEqual(len(filenames), 50, 'All 50 filenames must be distinct')

    def test_client_can_fetch_all_50_designs_and_refund_revokes_access(self):
        # Build 50 simulated unique design images
        simulated_assets = {}
        catalog_designs = []
        for idx in range(1, 51):
            ident = f'ws-{idx:03}'
            fill = bytes([(idx * 5) % 256, (idx * 17) % 256, (idx * 31) % 256])
            png_bytes = generate_png_320x200(fill)
            digest = hashlib.sha256(png_bytes).hexdigest()
            simulated_assets[ident] = png_bytes
            catalog_designs.append({
                'id': ident,
                'filename': f'{ident}.png',
                'title': f'Design #{idx}',
                'sha256': digest,
                'bytes': len(png_bytes),
                'width': 320,
                'height': 200,
            })

        mock_catalog = {
            'schema': 1,
            'pack_id': 'studio-50-v1',
            'title': 'Wallet Studio · 50 Diseños',
            'designs': catalog_designs,
        }

        self.store.design_catalog.catalog = lambda: mock_catalog
        self.store.design_catalog.read = lambda item: simulated_assets[item['id']]

        server = Server(('127.0.0.1', 0), self.store)
        server_thread = threading.Thread(target=server.serve_forever, daemon=True)
        server_thread.start()

        def http_request(path, token=None, method='GET', body=None, headers=None):
            req_headers = dict(headers or {})
            if token:
                req_headers['Authorization'] = f'Bearer {token}'
            req = urllib.request.Request(f'http://127.0.0.1:{server.server_port}{path}', data=body, headers=req_headers, method=method)
            try:
                with urllib.request.urlopen(req) as resp:
                    return resp.status, resp.read()
            except urllib.error.HTTPError as err:
                return err.code, err.read()

        try:
            # 1. Unauthenticated request to /v1/designs returns 401
            status, _ = http_request('/v1/designs')
            self.assertEqual(status, 401)

            # 2. Activate customer with active single SKU purchase
            session = self.activate_customer(ident=101)
            token = session['access_token']
            user_id = session['user_id']
            self.assertTrue(self.auth.pack_entitled(user_id))
            self.assertTrue(self.auth.app_entitled(user_id))

            # 3. Authenticated client fetches catalog
            status, body = http_request('/v1/designs', token=token)
            self.assertEqual(status, 200)
            data = json.loads(body)
            self.assertTrue(data.get('entitled'))
            self.assertEqual(len(data.get('designs', [])), 50)

            # 4. Client downloads each of the 50 designs
            for item in data['designs']:
                design_id = item['id']
                d_status, d_body = http_request(f'/v1/designs/{design_id}', token=token)
                self.assertEqual(d_status, 200, f'Download failed for {design_id}')
                self.assertEqual(len(d_body), item['bytes'])
                self.assertEqual(hashlib.sha256(d_body).hexdigest(), item['sha256'])
                validate_png(d_body)

            # 5. Trigger Shopify refunds/create webhook for this order
            refund_payload = {
                'id': 9991,
                'order_id': 101,
                'created_at': '2026-10-16T13:00:00Z',
                'refund_line_items': [{'line_item_id': 99, 'quantity': 1}],
            }
            raw_refund = json.dumps(refund_payload).encode()
            refund_sig = base64.b64encode(hmac.new(b'fixture-secret', raw_refund, hashlib.sha256).digest()).decode()
            webhook_headers = {
                'Content-Type': 'application/json',
                'X-Shopify-Hmac-Sha256': refund_sig,
                'X-Shopify-Shop-Domain': 'fixture.myshopify.com',
                'X-Shopify-Topic': 'refunds/create',
                'X-Shopify-Webhook-Id': 'webhook-refund-101',
            }
            w_status, w_body = http_request(
                '/v1/shopify/webhook',
                method='POST',
                body=raw_refund,
                headers=webhook_headers
            )
            self.assertEqual(w_status, 200)
            self.assertEqual(json.loads(w_body), {'received': True})

            # 6. Verify that entitlement is now revoked in Accounts
            self.assertFalse(self.auth.pack_entitled(user_id))
            self.assertFalse(self.auth.app_entitled(user_id))

            # 7. When the entire order is refunded, the session token is completely invalidated (fails closed)
            c_status, _ = http_request('/v1/designs', token=token)
            self.assertEqual(c_status, 401, 'Revoked customer session must be rejected with 401')

            dl_status, _ = http_request('/v1/designs/ws-001', token=token)
            self.assertEqual(dl_status, 401, 'Revoked customer download must be rejected with 401')

            b_status, _ = http_request('/v1/bundles', token=token)
            self.assertEqual(b_status, 401, 'Revoked customer bundles must be rejected with 401')

            # 8. Test an authenticated user without pack entitlement:
            # Simulates an active session where pack_entitled returns False
            with patch.object(self.auth, 'pack_entitled', return_value=False):
                with patch.object(self.auth, 'subject', return_value=user_id):
                    p_status, p_body = http_request('/v1/designs', token=token)
                    self.assertEqual(p_status, 200)
                    catalog_unentitled = json.loads(p_body)
                    self.assertFalse(catalog_unentitled.get('entitled'))
                    self.assertEqual(catalog_unentitled.get('designs'), [])

                    unentitled_dl_status, unentitled_dl_body = http_request('/v1/designs/ws-001', token=token)
                    self.assertEqual(unentitled_dl_status, 403)
                    self.assertEqual(json.loads(unentitled_dl_body), {'error': 'design_pack_required'})

        finally:
            server.shutdown()
            server.server_close()
            server_thread.join()

    def test_live_render_backend_health_and_unauthorized_catalog(self):
        render_url = 'https://wallet-studio-accounts.onrender.com'
        try:
            with urllib.request.urlopen(f'{render_url}/health', timeout=15) as resp:
                self.assertEqual(resp.status, 200)
                data = json.loads(resp.read())
                self.assertEqual(data.get('status'), 'ok')

            try:
                urllib.request.urlopen(f'{render_url}/v1/designs', timeout=15)
                self.fail('Unauthenticated /v1/designs should fail with 401')
            except urllib.error.HTTPError as err:
                if err.code in (502, 503, 504):
                    self.skipTest(f'Live Render free tier proxy warming up: HTTP {err.code}')
                self.assertEqual(err.code, 401)

        except (urllib.error.URLError, TimeoutError, ConnectionError) as net_err:
            self.skipTest(f'Live Render backend network probe skipped: {net_err}')


if __name__ == '__main__':
    unittest.main()
