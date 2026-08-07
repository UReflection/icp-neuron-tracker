# Identity Setup Guide

**Document Type:** Guide  
**Last Updated:** 2025-10-26  
**Maintained By:** U Reflection Design & Build Inc.

---

## Overview

ICP Neuron Tracker requires an Internet Computer identity to query neuron data. This guide covers complete identity setup using Secp256k1 elliptic curve keys.

Security Model: Tracker uses "hot key" pattern - read-only access without controller authority. You cannot transfer stake or change neuron settings with hot keys.

---

## Quick Start

### Generate Identity via Tracker

Simplest method for new users:
```bash
cargo run -- identity generate --name tracker-hotkey
```

Output:
```
U Reflection Design & Build Inc. - ICP Neuron Tracker
Identity Generator

Generating new Secp256k1 identity...

============================================================
Identity Generated Successfully
============================================================

Key Type: Secp256k1 (EC curve)

Principal:
  aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaa

PEM File:
  ./tracker-hotkey.pem
  Permissions: rw------- (owner only)

============================================================
Next Steps
============================================================

1. Add this principal as hot key to your neurons
2. Update config.toml with PEM file path
3. Verify: cargo run -- identity verify
4. Start tracking: cargo run
```

Follow the displayed instructions step by step.

---

## Key Type: Secp256k1

This tracker uses Secp256k1 elliptic curve identities.

### Why Secp256k1?

Industry Standard:
- Same cryptographic curve as Bitcoin and Ethereum
- Widely supported across blockchain ecosystems
- Proven security with decades of real-world usage

Internet Computer Support:
- Fully supported by IC protocol
- Compatible with governance canisters
- Works as hot key for neurons

Tool Compatibility:
- Works with hardware wallets
- Compatible with many blockchain tools
- Interoperable across ecosystems

### Key Type Comparison

| Feature | Secp256k1 | Ed25519 |
|---------|-----------|---------|
| Curve Type | ECDSA | EdDSA |
| Key Size | 256 bits | 256 bits |
| IC Support | Full | Full |
| Bitcoin/ETH Compatible | Yes | No |
| PEM Format | SEC1 (EC PRIVATE KEY) | PKCS#8 (PRIVATE KEY) |

Both work on Internet Computer. Secp256k1 chosen for broader ecosystem compatibility.

**Important:** This tracker generates Secp256k1 keys in SEC1 format (`BEGIN EC PRIVATE KEY`). Keys in PKCS#8 format (`BEGIN PRIVATE KEY`) are not compatible and will fail to load.

---

## Adding Hot Key to Neurons

For each neuron you want to track, add the generated principal as a hot key.

### Step-by-Step Process

Step 1: Get Your Principal
```bash
cargo run -- identity info
```

Copy the principal (format: xxxxx-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx-xxx)

Step 2: Open NNS Dapp

Navigate to: https://nns.ic0.app/neurons/

Step 3: Login

Login with your controller identity (the identity that owns the neurons, not the hot key).

Step 4: Select Neuron

Click on the neuron you want to track.

Step 5: Add Hot Key

- Scroll to "Hotkeys" section
- Click "Add Hotkey"
- Paste principal from step 1
- Confirm

Step 6: Repeat

Repeat steps 4-5 for each neuron you want to track.

Step 7: Verify
```bash
cargo run -- identity verify
```

All neurons should show "Authorized".

---

## Verification

Always verify your identity setup before tracking:
```bash
cargo run -- identity verify
```

### Success Output
```
U Reflection Design & Build Inc. - ICP Neuron Tracker
Identity Verification

PEM File: ./tracker-hotkey.pem ✓
Principal: aaaaa-aaaaa-aaaaa... ✓

Checking neuron authorization...
  Neuron 1000000000000000003... ✓ Authorized
  Neuron 10000000000000000002... ✓ Authorized
  Neuron 1000000000000000005... ✓ Authorized
  Neuron 10000000000000000001... ✓ Authorized

============================================================
All Neurons Authorized!
============================================================

Ready to track:
  cargo run
```

### Error: Not Authorized
```
Checking neuron authorization...
  Neuron 1000000000000000003... ✓ Authorized
  Neuron 10000000000000000002... ✗ Not authorized as hot key
  Neuron 1000000000000000005... ✓ Authorized

============================================================
Action Required
============================================================

Add principal as hot key to these neurons:
  - https://nns.ic0.app/neuron/10000000000000000002

Principal to add:
  aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaaaa-aaa
```

Solution: Follow the direct link, add hot key, verify again.

---

## Configuration

### Basic config.toml
```toml
[identity]
pem_file = "./tracker-hotkey.pem"

[ic]
ic_url = "https://ic0.app"
governance_canister = "rrkah-fqaaa-aaaaa-aaaaq-cai"

[neurons]
ids = [
    "1000000000000000003",
    "10000000000000000002",
    "1000000000000000005",
    "10000000000000000001"
]

[tracking]
history_file = "neuron_history.db"
snapshot_on_run = true
```

### Path Options

Relative Path (portable):
```toml
pem_file = "./tracker-hotkey.pem"
```

Absolute Path (secure):
```toml
# Unix/Linux/macOS
pem_file = "/home/username/.ic-identities/tracker-hotkey.pem"

# Windows
pem_file = "C:\\Users\\Username\\.ic-identities\\tracker-hotkey.pem"
```

---

## Alternative Methods

### Use Existing dfx Secp256k1 Identity

If you already have a dfx Secp256k1 identity:
```bash
# Export PEM
dfx identity export your-identity-name > tracker-identity.pem

# Check if Secp256k1
head -1 tracker-identity.pem
# Should show: -----BEGIN EC PRIVATE KEY-----
```

If shows "BEGIN PRIVATE KEY" (Ed25519), generate new Secp256k1:
```bash
cargo run -- identity generate --name tracker-hotkey
```

Update config.toml:
```toml
[identity]
pem_file = "./tracker-identity.pem"
```

### Generate Manually with OpenSSL

Advanced users:
```bash
# Generate Secp256k1 private key
openssl ecparam -name secp256k1 -genkey -noout -out tracker-identity.pem

# Verify
openssl ec -in tracker-identity.pem -text -noout
```

Get principal:
```bash
cargo run -- identity info
```

---

## PEM Format: SEC1 vs PKCS#8

This tracker requires Secp256k1 keys in **SEC1 format**.

### Format Comparison

| Format | Header | Usage | Compatibility |
|--------|--------|-------|---------------|
| SEC1 | `BEGIN EC PRIVATE KEY` | Elliptic curve keys | ic-agent Secp256k1Identity |
| PKCS#8 | `BEGIN PRIVATE KEY` | Generic private keys | ic-agent BasicIdentity, Ed25519 |

### Why SEC1?

The `ic-agent` Rust library's `Secp256k1Identity::from_pem()` method specifically expects SEC1 format. It parses the key using `SecretKey::from_sec1_der()`, which only accepts SEC1-encoded keys.

### Common Issues

**Problem:** Old tracker version generated PKCS#8
- **Error:** "PEM file uses PKCS#8 format"
- **Solution:** Regenerate identity with current version

**Problem:** dfx export may use different format
- **Error:** "Cannot parse PEM file"
- **Solution:** Use tracker's built-in generator

**Problem:** OpenSSL default may vary
- **Check format:** `head -1 identity.pem`
- **Required:** Must show `BEGIN EC PRIVATE KEY`

### Converting PKCS#8 to SEC1

If you have a PKCS#8 Secp256k1 key and need SEC1 format:

```bash
# Convert using OpenSSL
openssl ec -in pkcs8-key.pem -out sec1-key.pem

# Verify format
head -1 sec1-key.pem
# Should show: -----BEGIN EC PRIVATE KEY-----
```

**Note:** Only works for Secp256k1 keys. Ed25519 keys cannot be used with this tracker.

---

## Checking Your Identity Type

### Via PEM File Header
```bash
head -1 your-identity.pem
```

Secp256k1 shows:
```
-----BEGIN EC PRIVATE KEY-----
```

Ed25519 shows:
```
-----BEGIN PRIVATE KEY-----
```

### Via Tracker Command
```bash
cargo run -- identity info
```

Output includes key type.

---

## Security Best Practices

### File Permissions

PEM files contain private keys. Restrict access.

Unix/Linux/macOS:
```bash
# Set restrictive permissions
chmod 600 tracker-hotkey.pem

# Verify
ls -l tracker-hotkey.pem
# Should show: -rw-------
```

Windows:
```powershell
# Remove inheritance, grant only current user
icacls tracker-hotkey.pem /inheritance:r
icacls tracker-hotkey.pem /grant:r "%USERNAME%:(R,W)"
```

Tracker auto-sets these permissions when generating identities.

### Backup Strategy

Local Encrypted Backup:
```bash
# Using GPG
gpg -c tracker-hotkey.pem
# Creates tracker-hotkey.pem.gpg (encrypted)

# Restore
gpg -d tracker-hotkey.pem.gpg > tracker-hotkey.pem
chmod 600 tracker-hotkey.pem
```

Cloud Backup (Encrypted Only):
```bash
# Encrypt first
gpg -c tracker-hotkey.pem

# Upload encrypted file only
# NEVER upload unencrypted PEM to cloud
```

Paper Backup:
```bash
# Print PEM for offline storage
cat tracker-hotkey.pem

# Store in safe/vault
```

### Never Commit to Version Control

PEM files are in .gitignore by default.

Verify before committing:
```bash
git status
# Should NOT show .pem files

# Check .gitignore
cat .gitignore | grep pem
# Should show: *.pem
```

### If a key has already been committed

**Treat it as compromised from the moment it was committed.** Not from when you noticed, and
not from when you removed it. If the commit was ever pushed, or the repository was ever
cloned, forked or mirrored, the key is out. Removing it later does not retrieve it, and
neither does rewriting history — a rewrite changes what is easy to find, not what has already
been copied.

Rotation is therefore the fix. History is housekeeping, and it comes last.

**1. Generate a replacement.**

```bash
icp-neuron-tracker identity generate --name tracker-hotkey-new
```

**2. Re-register it, then remove the old one.** Add the new principal as a hot key on each
neuron in the [NNS dapp](https://nns.ic0.app), confirm the tracker reads your neurons with
it, and only then remove the exposed principal. Removing first leaves you locked out until
the new key propagates.

```bash
icp-neuron-tracker identity verify
```

**3. Confirm the exposed key controls nothing.** Check that its principal is no longer listed
as a hot key on any neuron. Until that is true, nothing else you do matters.

**4. Stop the leak from widening.** Delete the file from the working tree and confirm
`.gitignore` covers it, so the next commit does not re-add it.

**5. Only then consider history.** Rewriting is disruptive: it invalidates every commit SHA,
breaks tags, and breaks any reference to those SHAs recorded elsewhere. Weigh that against
the benefit, which is smaller than it looks given the key is already rotated. If you do
rewrite, use [`git-filter-repo`](https://github.com/newren/git-filter-repo); the older
built-in tooling is deprecated and is easy to use in ways that silently miss the object.

A hot key is read-and-vote only; it cannot move, dissolve or spawn stake. An exposed hot key
is a real incident and worth the rotation, but it is not a loss of funds.

### Storage Location

Recommended: Outside Project Directory
```bash
# Create dedicated identity directory
mkdir -p ~/.ic-identities
chmod 700 ~/.ic-identities

# Generate identity there
cd ~/.ic-identities
/path/to/tracker identity generate

# Update config.toml with absolute path
[identity]
pem_file = "/home/username/.ic-identities/tracker-hotkey.pem"
```

Benefits:
- Won't be accidentally committed
- Separate from code repository
- Easier to secure
- Can reuse across projects

---

## Hot Key vs Controller

### Hot Key (What Tracker Uses)

Capabilities:
- Query neuron state
- Read maturity balances
- View voting history
- Access neuron metadata

Restrictions:
- Cannot transfer stake
- Cannot dissolve neurons
- Cannot change settings
- Cannot spawn neurons
- Cannot disburse maturity
- Cannot vote on proposals

Security: If hot key compromised, attacker can only read data. Cannot steal stake.

### Controller (DO NOT USE)

Capabilities:
- Everything hot key can do
- Plus full control over neuron

Security: If controller compromised, attacker can steal all stake. NEVER use controller identity for tracking.

### Best Practice

Use dedicated hot key identity for tracking.

Controller: Secured in hardware wallet, rarely used  
Hot Key: Used by tracker, read-only access  
Separation: Compromise of tracker doesn't compromise stake

---

## Troubleshooting

### Error: PEM File Not Found
```
Error: PEM file not found
  Path: ./tracker-hotkey.pem
```

Solutions:

1. Check file exists:
```bash
   ls -la tracker-hotkey.pem
```

2. Check path in config.toml:
```toml
   [identity]
   pem_file = "./tracker-hotkey.pem"
```

3. Generate if missing:
```bash
   cargo run -- identity generate --name tracker-hotkey
```

### Error: Cannot Parse PEM File
```
Error: Cannot parse PEM file
```

Possible causes:

1. Wrong PEM format (PKCS#8 instead of SEC1):
```bash
   # Check header
   head -1 tracker-hotkey.pem

   # If shows "BEGIN PRIVATE KEY", it's PKCS#8 format
   # This tracker requires SEC1 format

   # Regenerate with correct format:
   mv tracker-hotkey.pem tracker-hotkey-old.pem
   cargo run -- identity generate
```

Common scenario: If you generated the identity with an older version of this tracker or using `dfx` with certain flags, it may be in PKCS#8 format. The tracker now exclusively uses SEC1 format for Secp256k1 keys.

2. File corrupted:
```bash
   # Check file is readable
   cat tracker-hotkey.pem

   # Should show: -----BEGIN EC PRIVATE KEY-----

   # If garbled, regenerate:
   cargo run -- identity generate --name tracker-hotkey-new
```

3. Not a PEM file:
```bash
   # Check file format
   file tracker-hotkey.pem

   # Should show: PEM certificate or ASCII text
```

### Error: Not Authorized
```
Neuron 1000000000000000003... ✗ Not authorized as hot key
```

Solutions:

1. Verify principal:
```bash
   cargo run -- identity info
```

2. Add hot key in NNS dapp:
   - Go to https://nns.ic0.app/neuron/1000000000000000003
   - Login with controller identity
   - Add Hotkey
   - Paste exact principal
   - Confirm

3. Wait for propagation:
```bash
   sleep 30
   cargo run -- identity verify
```

4. Check you're using controller identity in NNS (not the hot key)

### Error: File Already Exists
```
Error: File 'tracker-hotkey.pem' already exists
```

Solutions:

1. Use different name:
```bash
   cargo run -- identity generate --name tracker-hotkey-2
```

2. Remove existing file (backup first):
```bash
   cp tracker-hotkey.pem tracker-hotkey.pem.backup
   rm tracker-hotkey.pem
   cargo run -- identity generate --name tracker-hotkey
```

3. Use existing file:
```bash
   cargo run -- identity verify
```

### Error: Permission Denied
```
Error: Permission denied (os error 13)
```

Solutions:

1. Check directory permissions:
```bash
   touch test.txt && rm test.txt
```

2. Check file permissions:
```bash
   ls -l tracker-hotkey.pem
   chmod 600 tracker-hotkey.pem
```

3. Don't use sudo (creates root-owned files)

### Error: Invalid Principal Format
```
Error: Invalid neuron ID: not-a-number
```

Solution: Check config.toml neuron IDs are numeric strings:
```toml
[neurons]
ids = [
    "1000000000000000003",    # Correct
    "not-a-number",           # Wrong
]
```

Get neuron ID from NNS dapp URL:
```
https://nns.ic0.app/neuron/1000000000000000003
                            ^^^^^^^^^^^^^^^^^^^
                            This is the neuron ID
```

---

## Command Reference

### Identity Commands
```bash
# Generate new Secp256k1 identity
cargo run -- identity generate --name <name>

# Default name: tracker-hotkey
cargo run -- identity generate

# Verify identity and neuron authorization
cargo run -- identity verify

# Display current identity information
cargo run -- identity info

# Start tracking
cargo run
```

---

## Migration from Ed25519

If you have existing Ed25519 identities:

### Generate New Secp256k1
```bash
# Generate new Secp256k1 identity
cargo run -- identity generate --name tracker-secp256k1

# Add new principal as hot key to neurons
# Both Ed25519 and Secp256k1 hot keys can coexist

# Update config.toml
[identity]
pem_file = "./tracker-secp256k1.pem"
```

Benefits:
- Dedicated identity for tracking
- Clear separation of concerns
- Both hot keys work simultaneously

### Why Not Convert?

Cannot convert Ed25519 to Secp256k1:
- Different mathematical curves
- Different key generation algorithms
- No conversion path exists

Solution: Generate new identity, add as additional hot key.

---

## Advanced Topics

### Multiple Identities

Track different neuron sets with different identities:
```bash
# Generate identities
cargo run -- identity generate --name tracker-portfolio-1
cargo run -- identity generate --name tracker-portfolio-2

# Create separate configs
cp config.toml config-portfolio-1.toml
cp config.toml config-portfolio-2.toml

# Edit each config with different PEM and neurons
```

### Identity Rotation

Periodically rotate hot keys for security:
```bash
# Generate new identity
cargo run -- identity generate --name tracker-hotkey-2025

# Add new principal to neurons

# Update config.toml

# Verify
cargo run -- identity verify

# Remove old hot key from neurons after confirming
```

Recommended frequency: Annually or after suspected compromise.

---

## Related Documentation

- [Usage Guide](usage.md) - CSV formats, on-disk locations, reading projection output
- [Documentation Index](../README.md)

---

## Summary

Quick Setup:
1. cargo run -- identity generate
2. Add principal to neurons via NNS dapp
3. cargo run -- identity verify
4. cargo run

Key Points:
- Uses Secp256k1 (EC curve)
- Hot key equals read-only access
- Never use controller for tracking
- Keep PEM files secure
- Backup encrypted

Security:
- File permissions: 600
- Store outside repository
- Never commit to git
- Encrypted backups only

---

**U Reflection Design & Build Inc.**

Identity management made simple.  
Security through clear boundaries.

Last Updated: 2025-10-26
Version: 0.1.1