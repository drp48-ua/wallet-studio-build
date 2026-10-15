# PostgreSQL trust bundle

`supabase-root-2021.crt` is the public CA certificate downloaded on 2026-10-15 from the project's official Supabase dashboard, Database → Settings → SSL configuration → Download certificate. It contains no private key or project credential.

Downloaded-file SHA-256: `700723581420dd1ac98fd7e9ac529f0ef210eadcaf87fc868a3ad7d114c2f3b7`.

The Docker image combines Debian's public roots and this provider CA in a dedicated PostgreSQL trust bundle. `DATABASE_SSL_ROOT_CERT` selects that file; hostname and chain verification remain `verify-full`. The general HTTP trust store is unchanged. Renew this public CA through the official dashboard before its expiry (2031-04-26); never disable verification to work around certificate failures.

Provider instructions: https://supabase.com/docs/guides/database/psql and https://supabase.com/docs/guides/platform/ssl-enforcement.
