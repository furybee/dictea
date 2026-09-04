#!/usr/bin/env bash
#
# Generate the self-signed certificate used to sign the macOS builds.
#
# Why this exists
# ---------------
# An ad-hoc signed app has a designated requirement pinned to the hash of that
# exact binary:
#
#   $ codesign -d --requirements - Dictea.app
#   # designated => cdhash H"e234a650d6ced304a00c4d52fb4bc60a847ae0cc"
#
# macOS indexes TCC grants against that requirement, so every new build looks
# like a different app and all permissions are dropped. The microphone can
# re-prompt; Accessibility and Apple Events cannot, they just deny silently —
# which is why the automatic paste stops working after an update.
#
# Signing with a stable certificate anchors the requirement to the certificate
# instead, and the grants survive:
#
#   designated => identifier "com.dictea.app" and certificate leaf = H"..."
#
# What it does NOT do
# -------------------
# This is not a substitute for a paid Developer ID. Gatekeeper still treats the
# app as coming from an unidentified developer, exactly as it does today, and
# notarization stays out of reach. It only fixes the permission resets.
#
# Usage
# -----
#   ./scripts/generate-signing-cert.sh
#
# Run it once. Keep the .p12 somewhere safe and back it up: losing it means
# issuing a new certificate, which resets everyone's permissions one more time.

set -euo pipefail

IDENTITY="${DICTEA_SIGNING_IDENTITY:-Dictea Self Signed}"
OUT_DIR="${1:-$HOME/.dictea-signing}"
P12="$OUT_DIR/dictea-signing.p12"

if [ -f "$P12" ]; then
    echo "A certificate already exists at $P12"
    echo "Delete it first if you really mean to replace it — a new certificate"
    echo "resets the permissions of every existing install."
    exit 1
fi

mkdir -p "$OUT_DIR"
chmod 700 "$OUT_DIR"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 10 years: an expired certificate would mean issuing a new one, and another
# round of permission resets for everyone.
cat > "$WORK/cert.conf" <<EOF
[req]
distinguished_name = dn
x509_extensions = v3
prompt = no
[dn]
CN = $IDENTITY
[v3]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF

openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
    -keyout "$WORK/key.pem" -out "$WORK/cert.pem" \
    -config "$WORK/cert.conf" 2>/dev/null

PASSWORD="$(openssl rand -base64 24)"
openssl pkcs12 -export \
    -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
    -out "$P12" -passout "pass:$PASSWORD" -name "$IDENTITY" 2>/dev/null
chmod 600 "$P12"

echo "Certificate written to $P12"
echo "Identity: $IDENTITY"
echo
echo "Store it in the repository secrets — run these two commands:"
echo
echo "  gh secret set APPLE_CERTIFICATE < <(base64 -i '$P12')"
echo "  gh secret set APPLE_CERTIFICATE_PASSWORD --body '$PASSWORD'"
echo
echo "Then back up $P12 and the password somewhere durable (a password"
echo "manager): they cannot be recovered, and regenerating them costs every"
echo "user one more permission reset."
echo
echo "The release workflow signs only when APPLE_CERTIFICATE is present, so"
echo "nothing changes until both secrets are set."
