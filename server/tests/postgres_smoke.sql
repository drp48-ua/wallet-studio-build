-- SQL-engine smoke test only, not Python/HTTP/Storage/Brevo integration.
-- Entire script is one transaction. ROLLBACK is mandatory, including success.
-- No DROP/TRUNCATE/DELETE; synthetic user/email/order namespaces only.
BEGIN;
SELECT pg_advisory_xact_lock(6299748183224241);
CREATE SCHEMA IF NOT EXISTS wallet_private;
REVOKE ALL ON SCHEMA wallet_private FROM PUBLIC, anon, authenticated;
CREATE TABLE IF NOT EXISTS wallet_private.users(id TEXT PRIMARY KEY,email TEXT UNIQUE NOT NULL,salt BYTEA NOT NULL,password BYTEA NOT NULL);
CREATE TABLE IF NOT EXISTS wallet_private.sessions(digest TEXT PRIMARY KEY,user_id TEXT NOT NULL,expires BIGINT NOT NULL,device_id TEXT NOT NULL DEFAULT '',last_seen BIGINT NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS wallet_private.bundles(user_id TEXT NOT NULL,id TEXT NOT NULL,size BIGINT NOT NULL,PRIMARY KEY(user_id,id));
CREATE TABLE IF NOT EXISTS wallet_private.purchase_orders(id TEXT PRIMARY KEY,email TEXT NOT NULL,active BIGINT NOT NULL,updated DOUBLE PRECISION NOT NULL);
CREATE INDEX IF NOT EXISTS purchase_email ON wallet_private.purchase_orders(email,active);
CREATE TABLE IF NOT EXISTS wallet_private.webhook_events(id TEXT PRIMARY KEY,received BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS wallet_private.activation(email TEXT PRIMARY KEY,digest TEXT NOT NULL,expires BIGINT NOT NULL,attempts BIGINT NOT NULL,last_sent BIGINT NOT NULL);
CREATE TABLE IF NOT EXISTS wallet_private.computers(user_id TEXT NOT NULL,id TEXT NOT NULL,created BIGINT NOT NULL,last_seen BIGINT NOT NULL,peer_digest TEXT NOT NULL,PRIMARY KEY(user_id,id));
ALTER TABLE wallet_private.users ENABLE ROW LEVEL SECURITY;
ALTER TABLE wallet_private.sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE wallet_private.bundles ENABLE ROW LEVEL SECURITY;
ALTER TABLE wallet_private.purchase_orders ENABLE ROW LEVEL SECURITY;
ALTER TABLE wallet_private.webhook_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE wallet_private.activation ENABLE ROW LEVEL SECURITY;
ALTER TABLE wallet_private.computers ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON TABLE wallet_private.users,wallet_private.sessions,wallet_private.bundles,wallet_private.purchase_orders,wallet_private.webhook_events,wallet_private.activation,wallet_private.computers FROM PUBLIC,anon,authenticated;

CREATE TEMP TABLE wallet_studio_smoke_ids(user_id TEXT,email TEXT,order_id TEXT,digest TEXT) ON COMMIT DROP;
INSERT INTO wallet_studio_smoke_ids VALUES('smoke-'||md5(random()::text||clock_timestamp()::text),'smoke+'||md5(random()::text)||'@example.test','smoke-order-'||md5(random()::text),md5(random()::text)||md5(random()::text));
-- Dummy bytea is deliberately not a real password hash; validates types only.
INSERT INTO wallet_private.users SELECT user_id,email,decode(repeat('01',32),'hex'),decode(repeat('02',32),'hex') FROM wallet_studio_smoke_ids;
INSERT INTO wallet_private.purchase_orders SELECT order_id,email,1,1792065600.0 FROM wallet_studio_smoke_ids WHERE true
 ON CONFLICT(id) DO UPDATE SET email=excluded.email,active=excluded.active,updated=excluded.updated;
INSERT INTO wallet_private.activation SELECT email,repeat('a',64),4102444800,0,1792065600 FROM wallet_studio_smoke_ids WHERE true
 ON CONFLICT(email) DO UPDATE SET digest=excluded.digest,expires=excluded.expires,attempts=excluded.attempts,last_sent=excluded.last_sent;
UPDATE wallet_private.activation SET attempts=attempts+1 WHERE email=(SELECT email FROM wallet_studio_smoke_ids);
INSERT INTO wallet_private.webhook_events SELECT 'smoke-event-'||order_id,1792065600 FROM wallet_studio_smoke_ids;

-- Same table/order as production code, with three registrations under one lock.
INSERT INTO wallet_private.computers SELECT user_id,lpad(to_hex(i),64,'0'),1792065600,1792065600,repeat('f',64) FROM wallet_studio_smoke_ids CROSS JOIN generate_series(1,3) i WHERE true
 ON CONFLICT(user_id,id) DO UPDATE SET last_seen=excluded.last_seen,peer_digest=excluded.peer_digest;
-- A fourth registration must produce zero rows, without deleting prior slots.
INSERT INTO wallet_private.computers SELECT user_id,lpad('4',64,'0'),1792065600,1792065600,repeat('f',64) FROM wallet_studio_smoke_ids s
 WHERE (SELECT count(*) FROM wallet_private.computers c WHERE c.user_id=s.user_id)<3;
INSERT INTO wallet_private.sessions(digest,user_id,expires,device_id,last_seen) SELECT digest,user_id,4102444800,lpad('1',64,'0'),1792065600 FROM wallet_studio_smoke_ids;
INSERT INTO wallet_private.bundles SELECT user_id,repeat('b',64),123 FROM wallet_studio_smoke_ids;

-- Authoritative session/device/entitlement joins, not a client-declared count.
SELECT s.user_id,s.device_id,u.email FROM wallet_private.sessions s
 JOIN wallet_private.users u ON u.id=s.user_id
 JOIN wallet_private.computers c ON c.user_id=s.user_id AND c.id=s.device_id
 WHERE s.digest=(SELECT digest FROM wallet_studio_smoke_ids) AND s.expires>1792065600 AND s.last_seen>1789473600;
SELECT id,salt,password FROM wallet_private.users WHERE email=(SELECT email FROM wallet_studio_smoke_ids);
SELECT 1 FROM wallet_private.purchase_orders WHERE email=(SELECT email FROM wallet_studio_smoke_ids) AND active=1 LIMIT 1;
SELECT count(*),coalesce(sum(size),0) FROM wallet_private.bundles WHERE user_id=(SELECT user_id FROM wallet_studio_smoke_ids);

DO $$
BEGIN
 IF (SELECT count(*) FROM wallet_private.computers WHERE user_id=(SELECT user_id FROM wallet_studio_smoke_ids))<>3 THEN RAISE EXCEPTION 'synthetic_three_device_limit_failed'; END IF;
 IF (SELECT attempts FROM wallet_private.activation WHERE email=(SELECT email FROM wallet_studio_smoke_ids))<>1 THEN RAISE EXCEPTION 'synthetic_otp_attempt_update_failed'; END IF;
 IF has_schema_privilege('anon','wallet_private','USAGE') OR has_schema_privilege('authenticated','wallet_private','USAGE') THEN RAISE EXCEPTION 'private_schema_exposed'; END IF;
 IF EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='wallet_private' AND c.relname IN('users','sessions','bundles','purchase_orders','webhook_events','activation','computers') AND NOT c.relrowsecurity) THEN RAISE EXCEPTION 'private_table_without_rls'; END IF;
END $$;

-- Same-timestamp revocation: production serializes SELECT + update under lock.
UPDATE wallet_private.purchase_orders SET active=0 WHERE id=(SELECT order_id FROM wallet_studio_smoke_ids) AND updated=1792065600.0;
SELECT 'SQL_SMOKE_OK_ROLLBACK_NEXT' AS result;
ROLLBACK;
