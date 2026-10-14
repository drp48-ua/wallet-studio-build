"""Purchaser activation and computer licences; no customer identifiers in logs.

Entitlements come only from authenticated Shopify webhooks, never client claims.
SMTP is injected for tests; production always uses authenticated TLS delivery.
"""
import base64, hashlib, hmac, json, os, re, secrets, smtplib, ssl, time
from email.message import EmailMessage

DEVICE = re.compile(r'^[0-9a-f]{64}$')

class AccountError(ValueError):
    pass

class Accounts:
    def __init__(self, store, config=None, mailer=None):
        self.store=store; self.config=dict(os.environ if config is None else config); self.mailer=mailer or self.send_mail
        self.secret=self.config.get('WALLET_AUTH_SECRET','')
        self.shop=self.config.get('SHOPIFY_SHOP_DOMAIN','').lower()
        self.products=set(filter(None,self.config.get('SHOPIFY_PRODUCT_IDS','').split(',')))
        self.webhook_secret=self.config.get('SHOPIFY_WEBHOOK_SECRET','')
        with store.connect() as db:
            db.executescript('''
            CREATE TABLE IF NOT EXISTS purchase_orders(id TEXT PRIMARY KEY,email TEXT NOT NULL,active INTEGER NOT NULL,updated REAL NOT NULL);
            CREATE INDEX IF NOT EXISTS purchase_email ON purchase_orders(email,active);
            CREATE TABLE IF NOT EXISTS webhook_events(id TEXT PRIMARY KEY,received INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS activation(email TEXT PRIMARY KEY,digest TEXT NOT NULL,expires INTEGER NOT NULL,attempts INTEGER NOT NULL,last_sent INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS computers(user_id TEXT NOT NULL,id TEXT NOT NULL,created INTEGER NOT NULL,last_seen INTEGER NOT NULL,peer_digest TEXT NOT NULL,PRIMARY KEY(user_id,id));
            ''')
            columns={row[1] for row in db.execute('PRAGMA table_info(sessions)')}
            for name,definition in [('device_id',"TEXT NOT NULL DEFAULT ''"),('last_seen','INTEGER NOT NULL DEFAULT 0')]:
                if name not in columns: db.execute(f'ALTER TABLE sessions ADD COLUMN {name} {definition}')

    @property
    def ready(self):
        return len(self.secret)>=32 and bool(self.shop and self.products and self.webhook_secret)

    def require_ready(self):
        if not self.ready: raise AccountError('service_not_configured')

    def digest(self,value):
        return hmac.new(self.secret.encode(),value.encode(),hashlib.sha256).hexdigest()

    def entitled(self,db,email):
        return bool(db.execute('SELECT 1 FROM purchase_orders WHERE email=? AND active=1 LIMIT 1',(email,)).fetchone())

    def send_mail(self,email,code):
        cfg=self.config
        if not all(cfg.get(k) for k in ('SMTP_HOST','SMTP_USERNAME','SMTP_PASSWORD','SMTP_FROM')):
            raise AccountError('email_delivery_unavailable')
        message=EmailMessage();message['From']=cfg['SMTP_FROM'];message['To']=email;message['Subject']='Tu código de Wallet Studio'
        message.set_content(f'Tu código de activación de Wallet Studio es {code}. Caduca en 10 minutos. Si no lo solicitaste, ignora este mensaje.')
        with smtplib.SMTP(cfg['SMTP_HOST'],int(cfg.get('SMTP_PORT','587')),timeout=15) as smtp:
            smtp.ehlo();smtp.starttls(context=ssl.create_default_context());smtp.ehlo();smtp.login(cfg['SMTP_USERNAME'],cfg['SMTP_PASSWORD']);smtp.send_message(message)

    def start(self,email,purpose='activate'):
        self.require_ready();email=self.email(email);now=int(time.time())
        if purpose not in ('activate','recover'):raise AccountError('invalid_request')
        with self.store.lock, self.store.connect() as db:
            previous=db.execute('SELECT last_sent FROM activation WHERE email=?',(email,)).fetchone()
            if previous and now-previous[0]<60: return
            # Same response for unknown, existing and eligible addresses.
            exists=bool(db.execute('SELECT 1 FROM users WHERE email=?',(email,)).fetchone())
            if not self.entitled(db,email) or exists!=(purpose=='recover'): return
            code=f'{secrets.randbelow(1000000):06d}'
            db.execute('INSERT OR REPLACE INTO activation VALUES(?,?,?,?,?)',(email,self.digest(email+'|'+purpose+'|'+code),now+600,0,now))
        try: self.mailer(email,code)
        except Exception:
            with self.store.connect() as db: db.execute('DELETE FROM activation WHERE email=?',(email,))
            raise AccountError('email_delivery_unavailable') from None

    @staticmethod
    def email(value):
        if not isinstance(value,str):raise AccountError('invalid_account')
        value=value.strip().casefold()
        if not 3<=len(value)<=254 or not re.fullmatch(r'[^\s@]+@[^\s@]+\.[^\s@]+',value):raise AccountError('invalid_account')
        return value

    def activate(self,email,code,password,confirmation,device,peer):
        self.require_ready();email=self.email(email)
        if not isinstance(password,str) or not 12<=len(password)<=128 or password!=confirmation:raise AccountError('password_mismatch_or_length')
        if not isinstance(code,str) or not re.fullmatch('[0-9]{6}',code):raise AccountError('invalid_activation')
        with self.store.lock:
            # Failed attempts must commit even when authentication is rejected.
            with self.store.connect() as db:
                row=db.execute('SELECT digest,expires,attempts FROM activation WHERE email=?',(email,)).fetchone()
                if row:db.execute('UPDATE activation SET attempts=attempts+1 WHERE email=?',(email,))
                valid=row and row[1]>=int(time.time()) and row[2]<5 and hmac.compare_digest(row[0],self.digest(email+'|activate|'+code)) and self.entitled(db,email)
            if not valid:raise AccountError('invalid_activation')
            # Hold the store lock across consuming code + creating user + session.
            # Device validation precedes claiming; malformed requests cannot consume codes.
            if not isinstance(device,str) or not DEVICE.fullmatch(device):raise AccountError('invalid_device')
            try:self.store.create_user(email,password)
            except Exception:raise AccountError('invalid_activation') from None
            with self.store.connect() as db:db.execute('DELETE FROM activation WHERE email=?',(email,))
            return self.login(email,password,device,peer)

    def recover(self,email,code,password,confirmation,device,reset_devices,peer):
        self.require_ready();email=self.email(email)
        if not isinstance(password,str) or not 12<=len(password)<=128 or password!=confirmation:raise AccountError('password_mismatch_or_length')
        if not isinstance(device,str) or not DEVICE.fullmatch(device) or not isinstance(reset_devices,bool):raise AccountError('invalid_device')
        if not isinstance(code,str) or not re.fullmatch('[0-9]{6}',code):raise AccountError('invalid_activation')
        with self.store.lock:
            with self.store.connect() as db:
                row=db.execute('SELECT digest,expires,attempts FROM activation WHERE email=?',(email,)).fetchone()
                if row:db.execute('UPDATE activation SET attempts=attempts+1 WHERE email=?',(email,))
                valid=row and row[1]>=int(time.time()) and row[2]<5 and hmac.compare_digest(row[0],self.digest(email+'|recover|'+code)) and self.entitled(db,email)
            if not valid:raise AccountError('invalid_activation')
            salt=secrets.token_bytes(32);digest=hashlib.pbkdf2_hmac('sha256',password.encode(),salt,600_000)
            with self.store.connect() as db:
                db.execute('BEGIN IMMEDIATE')
                user=db.execute('SELECT id FROM users WHERE email=?',(email,)).fetchone()
                if not user:raise AccountError('invalid_activation')
                if not reset_devices and not db.execute('SELECT 1 FROM computers WHERE user_id=? AND id=?',(user[0],device)).fetchone() and db.execute('SELECT count(*) FROM computers WHERE user_id=?',(user[0],)).fetchone()[0]>=3:raise AccountError('device_limit_reached')
                db.execute('UPDATE users SET salt=?,password=? WHERE id=?',(salt,digest,user[0]))
                db.execute('DELETE FROM sessions WHERE user_id=?',(user[0],))
                if reset_devices:db.execute('DELETE FROM computers WHERE user_id=?',(user[0],))
                db.execute('DELETE FROM activation WHERE email=?',(email,))
            return self.login(email,password,device,peer)

    def login(self,email,password,device,peer):
        # Password verification and issuance share the same lock as recovery.
        # A password accepted before a concurrent reset cannot mint a new token
        # after that reset has revoked the previous credentials/sessions.
        with self.store.lock:return self._login_locked(email,password,device,peer)

    def _login_locked(self,email,password,device,peer):
        self.require_ready();email=self.email(email)
        if not isinstance(device,str) or not DEVICE.fullmatch(device):raise AccountError('invalid_device')
        # Credential verification never issues a usable production token on its own.
        row=self.store.verify_password(email,password)
        if not row:raise AccountError('invalid_credentials')
        now=int(time.time());token=secrets.token_hex(32)
        with self.store.lock,self.store.connect() as db:
            db.execute('BEGIN IMMEDIATE')
            if not self.entitled(db,email):raise AccountError('purchase_required')
            known=db.execute('SELECT 1 FROM computers WHERE user_id=? AND id=?',(row,device)).fetchone()
            if not known and db.execute('SELECT count(*) FROM computers WHERE user_id=?',(row,)).fetchone()[0]>=3:raise AccountError('device_limit_reached')
            db.execute('INSERT INTO computers VALUES(?,?,?,?,?) ON CONFLICT(user_id,id) DO UPDATE SET last_seen=excluded.last_seen,peer_digest=excluded.peer_digest',(row,device,now,now,self.digest(peer)))
            db.execute('DELETE FROM sessions WHERE expires<? OR (user_id=? AND device_id=?)',(now,row,device))
            db.execute('INSERT INTO sessions(digest,user_id,expires,device_id,last_seen) VALUES(?,?,?,?,?)',(hashlib.sha256(token.encode()).hexdigest(),row,now+86400*90,device,now))
        return {'access_token':token,'user_id':row,'expires_in':86400*90}

    def subject(self,token):
        if not self.ready or not isinstance(token,str) or not DEVICE.fullmatch(token):return None
        now=int(time.time());digest=hashlib.sha256(token.encode()).hexdigest()
        with self.store.connect() as db:
            row=db.execute('SELECT s.user_id,s.device_id,u.email FROM sessions s JOIN users u ON u.id=s.user_id JOIN computers c ON c.user_id=s.user_id AND c.id=s.device_id WHERE s.digest=? AND s.expires>? AND s.last_seen>?',(digest,now,now-86400*30)).fetchone()
            if not row or not self.entitled(db,row[2]):return None
            db.execute('UPDATE sessions SET last_seen=? WHERE digest=?',(now,digest))
            db.execute('UPDATE computers SET last_seen=? WHERE user_id=? AND id=?',(now,row[0],row[1]))
            return row[0]

    def devices(self,user):
        with self.store.connect() as db:return [{'id':r[0],'created':r[1],'last_seen':r[2]} for r in db.execute('SELECT id,created,last_seen FROM computers WHERE user_id=? ORDER BY created',(user,))]

    def revoke(self,user,device):
        with self.store.lock,self.store.connect() as db:
            db.execute('DELETE FROM sessions WHERE user_id=? AND device_id=?',(user,device));db.execute('DELETE FROM computers WHERE user_id=? AND id=?',(user,device))

    def webhook(self,raw,signature,shop,topic,event):
        self.require_ready()
        expected=base64.b64encode(hmac.new(self.webhook_secret.encode(),raw,hashlib.sha256).digest()).decode()
        if not isinstance(signature,str) or not hmac.compare_digest(expected,signature) or shop.lower()!=self.shop:raise AccountError('invalid_webhook_signature')
        if topic not in ('orders/paid','orders/updated','orders/cancelled'):raise AccountError('unsupported_webhook')
        if not isinstance(event,str) or not 1<=len(event)<=128:raise AccountError('invalid_webhook_id')
        body=json.loads(raw)
        # Shopify sends full order payloads for these topics. Partial refund events
        # are deliberately not used: orders/updated contains authoritative balances.
        from datetime import datetime
        date=datetime.fromisoformat(body['updated_at'].replace('Z','+00:00'))
        if date.tzinfo is None:raise AccountError('invalid_order_timestamp')
        updated=date.timestamp()
        email=self.email(body.get('email') or body.get('contact_email',''))
        ident=str(body['id'])
        if not ident.isdigit() or len(ident)>32:raise AccountError('invalid_order')
        eligible=[x for x in body.get('line_items',[]) if str(x.get('product_id')) in self.products and int(x.get('quantity',0))>0]
        refunded={}
        for refund in body.get('refunds',[]):
            for item in refund.get('refund_line_items',[]):refunded[str(item['line_item_id'])]=refunded.get(str(item['line_item_id']),0)+int(item.get('quantity',0))
        active=bool(eligible and body.get('financial_status') in ('paid','partially_refunded') and not body.get('cancelled_at') and topic!='orders/cancelled' and any(int(x['quantity'])>refunded.get(str(x['id']),0) for x in eligible))
        with self.store.lock,self.store.connect() as db:
            db.execute('BEGIN IMMEDIATE')
            if db.execute('SELECT 1 FROM webhook_events WHERE id=?',(event,)).fetchone():return
            old=db.execute('SELECT updated FROM purchase_orders WHERE id=?',(ident,)).fetchone()
            if not old or updated>old[0]:db.execute('INSERT OR REPLACE INTO purchase_orders VALUES(?,?,?,?)',(ident,email,int(active),updated))
            # Equal timestamp event conflicts fail closed rather than revive a refund.
            elif updated==old[0] and not active:db.execute('UPDATE purchase_orders SET active=0 WHERE id=?',(ident,))
            db.execute('INSERT INTO webhook_events VALUES(?,?)',(event,int(time.time())))
