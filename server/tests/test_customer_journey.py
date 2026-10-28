"""Purchaser journeys over real local HTTP with synthetic accounts and assets.

No production mail, payment, database, Apple installation or phone is used.
The mailbox and object-store adapters are the only replaced boundaries.
"""
import base64
import contextlib
import hashlib
import hmac
import io
import json
import pathlib
import sys
import threading
import unittest
import urllib.error
import urllib.request
import zipfile
from unittest.mock import patch

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import test_accounts as fixtures
from accounts import AccountError
from app import Server
from designs import DesignCatalog, EXPANDED_CATALOG_HEADER, NATIVE_VARIANT_CAPABILITY
from test_design_cache import BlobSpy
from test_designs import fixture_catalog
from test_native_catalog import native_metadata, png_at


class CustomerJourneyTests(unittest.TestCase):
    setUp = fixtures.AccountTests.setUp
    tearDown = fixtures.AccountTests.tearDown
    order = fixtures.AccountTests.order
    webhook = fixtures.AccountTests.webhook

    @contextlib.contextmanager
    def http(self):
        server = Server(('127.0.0.1', 0), self.store)
        server.daemon_threads = False
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()

        def call(path, method='GET', body=None, token=None, headers=None):
            request_headers = dict(headers or {})
            if token:
                request_headers['Authorization'] = 'Bearer ' + token
            raw = json.dumps(body).encode() if isinstance(body, dict) else body
            request = urllib.request.Request(
                f'http://127.0.0.1:{server.server_port}{path}',
                data=raw, headers=request_headers, method=method)
            try:
                with urllib.request.urlopen(request, timeout=15) as response:
                    return response.status, response.read()
            except urllib.error.HTTPError as error:
                return error.code, error.read()

        try:
            yield call
        finally:
            server.shutdown()
            server.server_close()
            thread.join(5)
            self.assertFalse(thread.is_alive())

    def paid(self, call, email, ident):
        raw = json.dumps(self.order(ident=ident, email=email)).encode()
        headers = {
            'X-Shopify-Hmac-Sha256': base64.b64encode(hmac.new(
                b'fixture-secret', raw, hashlib.sha256).digest()).decode(),
            'X-Shopify-Shop-Domain': 'fixture.myshopify.com',
            'X-Shopify-Topic': 'orders/paid',
            'X-Shopify-Webhook-Id': f'synthetic-paid-{ident}',
        }
        self.assertEqual(call('/v1/shopify/webhook', 'POST', raw, headers=headers)[0], 200)

    def activate_http(self, call, email, device):
        self.assertEqual(call('/v1/auth/start', 'POST', {'email': email})[0], 200)
        target, code = self.mail[-1]
        self.assertEqual(target, email)
        status, raw = call('/v1/auth/activate', 'POST', {
            'email': email, 'code': code, 'password': 'synthetic-strong-password',
            'password_confirmation': 'synthetic-strong-password', 'device_id': device})
        self.assertEqual(status, 200)
        return json.loads(raw)['access_token']

    def test_purchase_activation_native_library_backup_second_computer_and_isolation(self):
        data = png_at(1536, 969)
        catalog = fixture_catalog(data)
        catalog['designs'] = [dict(native_metadata(data)[0], id=f'ws-{i:03}',
                                   filename=f'ws-{i:03}.png') for i in range(1, 57)]
        path = pathlib.Path(self.tmp.name) / 'synthetic-native-catalog.json'
        path.write_text(json.dumps(catalog), encoding='utf-8')
        assets = DesignCatalog({'WALLET_DESIGN_CATALOG': str(path),
                                'SUPABASE_URL': 'https://fixture.supabase.co',
                                'SUPABASE_SERVICE_ROLE_KEY': 'synthetic-only'})
        spy = BlobSpy(data)
        assets.blobs = lambda: spy
        self.store.design_catalog = assets
        with self.http() as call:
            self.assertEqual(call('/v1/auth/register', 'POST', {})[0], 403)
            self.assertEqual(call('/v1/bundles')[0], 401)
            self.assertEqual(call('/v1/auth/start', 'POST', {'email': 'unpaid@example.test'})[0], 200)
            self.assertFalse(self.mail)
            self.paid(call, 'first@example.test', 201)
            first = self.activate_http(call, 'first@example.test', '1' * 64)
            self.paid(call, 'other@example.test', 202)
            other = self.activate_http(call, 'other@example.test', '2' * 64)
            status, account = call('/v1/auth/session', token=first)
            self.assertEqual(status, 200)
            self.assertTrue(json.loads(account)['app_licensed'])
            headers = {EXPANDED_CATALOG_HEADER: NATIVE_VARIANT_CAPABILITY}
            status, raw = call('/v1/designs', token=first, headers=headers)
            self.assertEqual(status, 200)
            designs = json.loads(raw)['designs']
            self.assertEqual(len(designs), 56)
            self.assertTrue(all((item['width'], item['height']) == (1536, 969) for item in designs))
            self.assertEqual(call('/v1/designs/ws-001', token=first, headers=headers), (200, data))
            archive = io.BytesIO()
            with zipfile.ZipFile(archive, 'w') as zipped:
                zipped.writestr('fixture.json', '{"synthetic":true}')
            payload = archive.getvalue()
            digest = hashlib.sha256(payload).hexdigest()
            route = '/v1/bundles/' + digest
            self.assertEqual(call(route, 'PUT', payload, first)[0], 201)
            self.assertEqual(call(route, 'PUT', payload, first)[0], 200)
            self.assertEqual(call(route, token=other)[0], 404)
            self.assertEqual(json.loads(call('/v1/bundles', token=other)[1])['bundles'], [])
            status, raw = call('/v1/auth/login', 'POST', {
                'email': 'first@example.test', 'password': 'synthetic-strong-password', 'device_id': '3' * 64})
            self.assertEqual(status, 200)
            second_pc = json.loads(raw)['access_token']
            self.assertEqual(call(route, token=second_pc), (200, payload))
            self.assertEqual(call('/v1/designs/ws-001', token=second_pc, headers=headers), (200, data))
            self.assertEqual(spy.object_bytes, len(data), 'second computer reuses the verified server asset cache')
            self.assertEqual(call('/v1/auth/logout', 'POST', {}, first)[0], 200)
            self.assertEqual(call(route, token=first)[0], 401)
            self.assertEqual(call(route, token=second_pc), (200, payload))

    def test_failed_older_mail_cannot_delete_a_newer_successfully_delivered_code(self):
        self.webhook(self.order())
        started = threading.Event()
        release = threading.Event()
        results = {}

        def mailer(email, code):
            if code == '123456':
                started.set()
                if not release.wait(10):
                    raise RuntimeError('synthetic mail worker timed out')
                raise RuntimeError('synthetic first delivery failed')
            self.mail.append((email, code))

        def older_request():
            try:
                self.auth.start('buyer@example.test')
            except AccountError as error:
                results['older_error'] = str(error)

        self.auth.mailer = mailer
        with patch('accounts.time.time', return_value=2_000_000_000) as clock, \
                patch('accounts.secrets.randbelow', side_effect=[123456, 654321]):
            thread = threading.Thread(target=older_request)
            thread.start()
            try:
                self.assertTrue(started.wait(5))
                clock.return_value += 61
                self.auth.start('buyer@example.test')
            finally:
                release.set()
                thread.join(10)
            self.assertFalse(thread.is_alive())
            self.assertEqual(results.get('older_error'), 'email_delivery_unavailable')
            self.assertEqual(self.mail, [('buyer@example.test', '654321')])
            result = self.auth.activate('buyer@example.test', '654321',
                                       'synthetic-strong-password', 'synthetic-strong-password', '1' * 64, 'fixture')
            self.assertEqual(self.auth.subject(result['access_token']), result['user_id'])


if __name__ == '__main__':
    unittest.main()
