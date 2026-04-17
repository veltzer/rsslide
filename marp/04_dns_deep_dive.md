---
tags:
  - networking:dns
  - networking:protocols
level: beginner
category: networking
audience:
  - audiences:developers
  - audiences:devops
  - audiences:sysadmins

---
# DNS Deep Dive
## Understanding the Domain Name System

---

## What is DNS?

- Domain Name System: the "phonebook" of the internet
- Translates human-readable domain names to IP addresses
- Distributed, hierarchical database system
- Critical infrastructure -- without DNS, the internet is nearly unusable
- Defined in RFC 1034 and RFC 1035 (1987), with many subsequent updates

```misc
User types:    www.example.com
DNS resolves:  www.example.com → 93.184.216.34
Browser uses:  93.184.216.34 to establish TCP connection
```

---

## The DNS Hierarchy

DNS is organized as an inverted tree structure with the root at the top.

---

## The DNS Hierarchy

![the_dns_hierarchy](svg/courses/networking/networking-basics/04_dns_deep_dive/the_dns_hierarchy.svg)

---

## The DNS Hierarchy

Each level is called a "zone" and is managed by different organizations:
- **Root zone**: managed by ICANN, served by 13 root server clusters (a.root-servers.net through m.root-servers.net)
- **TLD zone**: managed by registries (.com by Verisign, .org by PIR, ccTLDs by country organizations)
- **Second-level domains**: managed by domain owners (you!)

---

## DNS Root Servers

There are 13 logical root server addresses (A through M), but hundreds of physical servers worldwide using anycast routing.

| Letter | Operator | Locations |
|--------|----------|-----------|
| A | Verisign | Distributed |
| B | USC-ISI | Los Angeles |
| C | Cogent | Distributed |
| D | University of Maryland | College Park |
| E | NASA Ames | Mountain View |
| F | ISC | Distributed (100+) |
| J | Verisign | Distributed (100+) |
| K | RIPE NCC | Distributed |
| L | ICANN | Distributed |
| M | WIDE Project | Distributed |

```bash
# Query a root server directly
dig @a.root-servers.net . NS

# See all root server IPs
dig . NS +short
```

---

## DNS Record Types Overview

DNS stores various types of records, each serving a different purpose.

| Record Type | Purpose | Example |
|-------------|---------|---------|
| A | IPv4 address | example.com → 93.184.216.34 |
| AAAA | IPv6 address | example.com → 2606:2800:220:1:... |
| CNAME | Canonical name (alias) | www.example.com → example.com |
| MX | Mail exchange | example.com → mail.example.com |
| TXT | Text data | SPF, DKIM, domain verification |
| NS | Name server | example.com → ns1.example.com |
| SOA | Start of authority | Zone metadata |
| PTR | Reverse DNS | IP → domain name |
| SRV | Service locator | _sip._tcp.example.com |

---

## A and AAAA Records

The most fundamental DNS records -- they map domain names to IP addresses.

**A Record** (IPv4):
```bash
$ dig example.com A +short
93.184.216.34

$ dig example.com A

;; ANSWER SECTION:
example.com.        3600    IN    A    93.184.216.34
```

**AAAA Record** (IPv6):
```bash
$ dig example.com AAAA +short
2606:2800:220:1:248:1893:25c8:1946

$ dig example.com AAAA

;; ANSWER SECTION:
example.com.        3600    IN    AAAA    2606:2800:220:1:248:1893:25c8:1946
```

A domain can have multiple A records for load balancing (round-robin DNS):
```bash
$ dig google.com A +short
142.250.80.46
142.250.80.78
142.250.80.110
```

---

## CNAME Records

CNAME (Canonical Name) creates an alias from one domain name to another.

---

## CNAME Records

![cname_records](svg/courses/networking/networking-basics/04_dns_deep_dive/cname_records.svg)

---

## CNAME Records

```bash
$ dig www.example.com
;; ANSWER SECTION:
www.example.com.    3600    IN    CNAME   example.com.
example.com.        3600    IN    A       93.184.216.34
```
**Important CNAME rules:**
- A CNAME cannot coexist with other record types for the same name
- A CNAME cannot be at the zone apex (e.g., example.com itself)
- Some providers offer "ALIAS" or "ANAME" as a workaround for apex CNAMEs

---

## MX Records

MX (Mail Exchange) records direct email for a domain to the correct mail servers.
```bash
$ dig example.com MX
;; ANSWER SECTION:
example.com.    3600    IN    MX    10 mail1.example.com.
example.com.    3600    IN    MX    20 mail2.example.com.
example.com.    3600    IN    MX    30 mail3.example.com.
```
The number before the server name is the **priority** (lower = higher priority).

---

## MX Records

![mx_records](svg/courses/networking/networking-basics/04_dns_deep_dive/mx_records.svg)

---

## MX Records

If mail1 is unreachable, the sender automatically tries mail2, then mail3.

---

## TXT Records

TXT records hold arbitrary text data. Widely used for domain verification and email security.

**Common uses:**

1. **SPF** (Sender Policy Framework) -- specifies which servers can send email for your domain:
```bash
$ dig example.com TXT +short
"v=spf1 include:_spf.google.com ~all"
```

1. **DKIM** (DomainKeys Identified Mail) -- email authentication via cryptographic signatures:
```bash
$ dig google._domainkey.example.com TXT +short
"v=DKIM1; k=rsa; p=MIGfMA0GCSqGSIb3DQEBAQUAA..."
```

1. **DMARC** -- policy for handling email that fails SPF/DKIM:
```bash
$ dig _dmarc.example.com TXT +short
"v=DMARC1; p=reject; rua=mailto:dmarc@example.com"
```

1. **Domain verification** (Google, Microsoft, Let's Encrypt):
```bash
$ dig example.com TXT +short
"google-site-verification=abc123..."
```

---

## NS Records

NS (Name Server) records delegate a DNS zone to specific authoritative name servers.
```bash
$ dig example.com NS
;; ANSWER SECTION:
example.com.    86400    IN    NS    a.iana-servers.net.
example.com.    86400    IN    NS    b.iana-servers.net.
```
NS records form the delegation chain from root to your domain:

---

## NS Records

![ns_records](svg/courses/networking/networking-basics/04_dns_deep_dive/ns_records.svg)

---

## SOA Record

SOA (Start of Authority) contains administrative information about a DNS zone.

```bash
$ dig example.com SOA

;; ANSWER SECTION:
example.com.  3600  IN  SOA  ns1.example.com. admin.example.com. (
                            2024010101  ; Serial number
                            7200        ; Refresh (2 hours)
                            3600        ; Retry (1 hour)
                            1209600     ; Expire (14 days)
                            86400       ; Minimum TTL (1 day)
                            )
```

| Field | Meaning |
|-------|---------|
| ns1.example.com. | Primary name server |
| admin.example.com. | Admin email (@ replaced with .) |
| Serial | Zone version (often YYYYMMDDNN format) |
| Refresh | How often secondaries check for updates |
| Retry | Retry interval if refresh fails |
| Expire | When secondaries stop serving if primary unreachable |
| Minimum TTL | Default TTL for negative caching |

---

## PTR Records (Reverse DNS)

PTR records map IP addresses back to domain names. Used for reverse DNS lookups.

IPv4 reverse lookups use the special domain `in-addr.arpa` with octets reversed:

```bash
# Forward lookup
$ dig example.com A +short
93.184.216.34

# Reverse lookup (note reversed octets)
$ dig -x 93.184.216.34
;; ANSWER SECTION:
34.216.184.93.in-addr.arpa. 3600 IN PTR example.com.

# Shorthand using -x flag
$ dig -x 8.8.8.8 +short
dns.google.
```

**Why reverse DNS matters:**
- Email servers check PTR records to verify sending servers (anti-spam)
- Logging and monitoring tools resolve IPs to names for readability
- Some services reject connections from IPs without valid PTR records

---

## SRV Records

SRV (Service) records specify the location of services, including port numbers.

Format: `_service._protocol.name TTL IN SRV priority weight port target`

```bash
$ dig _sip._tcp.example.com SRV

;; ANSWER SECTION:
_sip._tcp.example.com. 3600 IN SRV 10 60 5060 sip1.example.com.
_sip._tcp.example.com. 3600 IN SRV 10 40 5060 sip2.example.com.
_sip._tcp.example.com. 3600 IN SRV 20 0  5060 sip3.example.com.
```

| Field | Meaning |
|-------|---------|
| Priority | Lower = preferred (like MX) |
| Weight | Load balancing among same-priority targets |
| Port | TCP/UDP port for the service |
| Target | Hostname providing the service |

**Common SRV uses:**
- SIP (VoIP): `_sip._tcp`
- XMPP (chat): `_xmpp-client._tcp`
- LDAP: `_ldap._tcp`
- Kubernetes service discovery

---

## Recursive vs Iterative Resolution

DNS resolution can happen in two modes:
**Recursive Resolution** -- the resolver does all the work:

---

## Recursive vs Iterative Resolution

![recursive_vs_iterative_resolution](svg/courses/networking/networking-basics/04_dns_deep_dive/recursive_vs_iterative_resolution.svg)

---

## Recursive vs Iterative Resolution

**Iterative Resolution** -- each server returns the next server to ask:
```misc
Client asks Root:    "Where is www.example.com?"
Root responds:       "I don't know, but ask .com NS at 192.5.6.30"
Client asks .com:    "Where is www.example.com?"
.com responds:       "I don't know, but ask example.com NS at 93.184.216.34"
Client asks example.com NS: "Where is www.example.com?"
example.com responds:       "Here: 93.184.216.34"
```
In practice, your computer uses recursive resolution (asking your configured DNS resolver), and the resolver uses iterative resolution to walk the hierarchy.

---

## DNS Resolution: Step by Step

What happens when you type `www.example.com` in your browser:

---

## DNS Resolution: Step by Step

![dns_resolution_step_by_step](svg/courses/networking/networking-basics/04_dns_deep_dive/dns_resolution_step_by_step.svg)

---

## DNS Resolution: Step by Step

```bash
# Trace the full resolution path
$ dig +trace www.example.com
; <<>> DiG 9.18.1 <<>> +trace www.example.com
;; global options: +cmd
.                       518400  IN      NS      a.root-servers.net.
com.                    172800  IN      NS      a.gtld-servers.net.
example.com.            172800  IN      NS      a.iana-servers.net.
www.example.com.        86400   IN      A       93.184.216.34
```

---

## DNS Caching and TTL

Every DNS record has a TTL (Time To Live) that controls how long it can be cached.

```bash
$ dig example.com A

;; ANSWER SECTION:
example.com.        3600    IN    A    93.184.216.34
                    ^^^^
                    TTL = 3600 seconds (1 hour)
```

**TTL trade-offs:**

| Short TTL (60-300s) | Long TTL (3600-86400s) |
|---------------------|------------------------|
| Quick DNS changes propagate fast | Reduced DNS query load |
| Higher DNS query volume | Faster resolution (cached) |
| Good for failover scenarios | Changes take longer to propagate |
| Useful before migrations | Better for stable services |

**Caching layers:**
1. Browser cache (Chrome: `chrome://net-internals/#dns`)
1. OS cache (`systemd-resolved`, `nscd`, Windows DNS Client)
1. Recursive resolver cache (ISP, 8.8.8.8, 1.1.1.1)

```bash
# View systemd-resolved cache statistics
$ resolvectl statistics

# Flush DNS cache on Linux (systemd)
$ sudo resolvectl flush-caches

# Flush DNS cache on macOS
$ sudo dscacheutil -flushcache; sudo killall -HUP mDNSResponder

# Flush DNS cache on Windows
> ipconfig /flushdns
```

---

## Negative Caching

DNS also caches negative results -- when a domain does not exist.

```bash
$ dig nonexistent.example.com A

;; AUTHORITY SECTION:
example.com.  86400  IN  SOA  ns1.example.com. admin.example.com. (
                            ...
                            86400  ; Minimum TTL ← used for negative caching
                            )

;; ->>HEADER<<- opcode: QUERY, status: NXDOMAIN
```

The SOA minimum TTL field determines how long the "does not exist" (NXDOMAIN) response is cached. This prevents repeated queries for non-existent domains.

---

## Configuring DNS on Linux

### /etc/resolv.conf

The primary DNS configuration file on Linux:

```bash
$ cat /etc/resolv.conf

# DNS resolver configuration
nameserver 8.8.8.8          # Primary DNS server (Google)
nameserver 8.8.4.4          # Secondary DNS server
nameserver 1.1.1.1          # Tertiary DNS server (Cloudflare)
search example.com corp.example.com  # Search domains
options timeout:2 attempts:3 rotate  # Options
```

| Directive | Purpose |
|-----------|---------|
| nameserver | IP of DNS resolver (max 3) |
| search | Domains appended to short hostnames |
| domain | Default search domain (alternative to search) |
| options timeout:N | Query timeout in seconds |
| options attempts:N | Number of retries |
| options rotate | Round-robin among nameservers |

### /etc/nsswitch.conf

Controls the order of name resolution:
```bash
$ grep hosts /etc/nsswitch.conf
hosts: files dns mymachines myhostname
#      ^     ^
#      |     └── Use DNS (/etc/resolv.conf)
#      └── Check /etc/hosts first
```

---

## systemd-resolved

Modern Linux distributions often use systemd-resolved as a local DNS stub resolver.

```bash
# Check resolved status
$ resolvectl status
Global
       Protocols: +LLMNR +mDNS -DNSOverTLS DNSSEC=no/unsupported
resolv.conf mode: stub

Link 2 (eth0)
    Current Scopes: DNS LLMNR/IPv4 LLMNR/IPv6
         Protocols: +DefaultRoute +LLMNR -mDNS -DNSOverTLS
Current DNS Server: 8.8.8.8
       DNS Servers: 8.8.8.8 8.8.4.4

# Query using resolvectl
$ resolvectl query example.com
example.com: 93.184.216.34
             2606:2800:220:1:248:1893:25c8:1946

# View cache statistics
$ resolvectl statistics
DNSSEC supported by current servers: no
Transactions
  Current:        0
  Total:       1234
Cache
  Current Size: 89
  Hits:        567
  Misses:      667
```

When systemd-resolved is active, `/etc/resolv.conf` typically points to `127.0.0.53`.

---

## Practical DNS Tools: dig

`dig` (Domain Information Groper) is the most powerful DNS query tool.

```bash
# Basic query
$ dig example.com

# Query specific record type
$ dig example.com MX
$ dig example.com AAAA
$ dig example.com TXT

# Short output
$ dig example.com +short
93.184.216.34

# Query a specific DNS server
$ dig @8.8.8.8 example.com
$ dig @1.1.1.1 example.com

# Trace full resolution path
$ dig +trace example.com

# Show only answer section
$ dig example.com +noall +answer

# Reverse DNS lookup
$ dig -x 8.8.8.8

# Query with TCP instead of UDP
$ dig +tcp example.com

# Check DNSSEC
$ dig example.com +dnssec

# Batch queries from file
$ dig -f domains.txt +short
```

---

## Practical DNS Tools: nslookup and host

### nslookup (older, interactive tool)

```bash
# Simple lookup
$ nslookup example.com
Server:    8.8.8.8
Address:   8.8.8.8#53

Non-authoritative answer:
Name:    example.com
Address: 93.184.216.34

# Query specific record type
$ nslookup -type=MX example.com

# Use a specific DNS server
$ nslookup example.com 1.1.1.1

# Interactive mode
$ nslookup
> set type=AAAA
> example.com
> exit
```

### host (simple, scriptable)

```bash
# Simple lookup
$ host example.com
example.com has address 93.184.216.34
example.com has IPv6 address 2606:2800:220:1:248:1893:25c8:1946
example.com mail is handled by 0 .

# Reverse lookup
$ host 8.8.8.8
8.8.8.8.in-addr.arpa domain name pointer dns.google.

# Specific record type
$ host -t MX example.com
$ host -t NS example.com
```

---

## DNSSEC: Securing DNS

DNSSEC (DNS Security Extensions) adds cryptographic signatures to DNS records to prevent tampering.
**The problem DNSSEC solves:**

---

## DNSSEC: Securing DNS

![dnssec_securing_dns](svg/courses/networking/networking-basics/04_dns_deep_dive/dnssec_securing_dns.svg)

---

## DNSSEC: Securing DNS

**DNSSEC record types:**
- **RRSIG**: Contains the signature for a record set
- **DNSKEY**: Contains the public key used to verify signatures
- **DS**: Delegation Signer -- links parent zone to child zone keys
- **NSEC/NSEC3**: Proves a record does not exist (authenticated denial)
```bash
# Check if a domain uses DNSSEC
$ dig example.com +dnssec +short
93.184.216.34
A 13 2 86400 20240201000000 20240115000000 12345 example.com. <signature>
# Validate DNSSEC chain
$ dig +sigchase +trusted-key=/etc/trusted-key.key example.com
```

---

## DNS over HTTPS (DoH) and DNS over TLS (DoT)

Traditional DNS queries are sent in plaintext over UDP port 53, allowing eavesdropping. DoH and DoT encrypt DNS queries.

| Feature | Traditional DNS | DNS over TLS (DoT) | DNS over HTTPS (DoH) |
|---------|----------------|--------------------|-----------------------|
| Port | 53 (UDP/TCP) | 853 (TCP) | 443 (TCP) |
| Encryption | None | TLS | HTTPS (TLS) |
| Privacy | Low | High | High |
| Blockable | Easily | Moderate | Hard (same as HTTPS) |
| Standard | RFC 1035 | RFC 7858 | RFC 8484 |

**Popular DoH/DoT providers:**

| Provider | DoH URL | DoT Server |
|----------|---------|------------|
| Google | https://dns.google/dns-query | dns.google |
| Cloudflare | https://cloudflare-dns.com/dns-query | 1dot1dot1dot1.cloudflare-dns.com |
| Quad9 | https://dns.quad9.net/dns-query | dns.quad9.net |

```bash
# Test DoH with curl
$ curl -s -H 'Accept: application/dns-json' \
    'https://cloudflare-dns.com/dns-query?name=example.com&type=A' | python -m json.tool

# Configure systemd-resolved for DoT
# Edit /etc/systemd/resolved.conf:
[Resolve]
DNS=1.1.1.1#cloudflare-dns.com
DNSOverTLS=yes
```

---

## Common DNS Issues and Debugging

### Problem 1: DNS resolution failure

```bash
# Symptom: "Could not resolve host"
$ curl https://example.com
curl: (6) Could not resolve host: example.com

# Step 1: Check /etc/resolv.conf
$ cat /etc/resolv.conf

# Step 2: Test with a known-good resolver
$ dig @8.8.8.8 example.com

# Step 3: Check if it's a specific record issue
$ dig example.com A
$ dig example.com AAAA

# Step 4: Check if it's a local cache issue
$ sudo resolvectl flush-caches
$ dig example.com
```

---

## Common DNS Issues (continued)

### Problem 2: Slow DNS resolution

```bash
# Measure DNS lookup time
$ time dig example.com +short
93.184.216.34
real    0m0.045s

# If slow, check each resolver
$ dig @8.8.8.8 example.com | grep "Query time"
;; Query time: 12 msec

$ dig @1.1.1.1 example.com | grep "Query time"
;; Query time: 5 msec

# Check for packet loss to DNS server
$ ping -c 10 8.8.8.8
```

### Problem 3: DNS propagation delay

After changing DNS records, different resolvers may return old values due to caching.

```bash
# Check current TTL to know when caches will expire
$ dig example.com A | grep -A1 "ANSWER SECTION"
example.com.    1800    IN    A    93.184.216.34

# Query multiple public resolvers to check propagation
$ dig @8.8.8.8 example.com +short
$ dig @1.1.1.1 example.com +short
$ dig @9.9.9.9 example.com +short
$ dig @208.67.222.222 example.com +short
```

---

## DNS Zones and Zone Files

A zone file is the actual data file on an authoritative DNS server.

```misc
; Zone file for example.com
$TTL 86400
@   IN  SOA   ns1.example.com. admin.example.com. (
              2024010101  ; Serial
              7200        ; Refresh
              3600        ; Retry
              1209600     ; Expire
              86400 )     ; Minimum TTL

; Name servers
@       IN  NS    ns1.example.com.
@       IN  NS    ns2.example.com.

; A records
@       IN  A     93.184.216.34
www     IN  A     93.184.216.34
mail    IN  A     93.184.216.50

; AAAA records
@       IN  AAAA  2606:2800:220:1:248:1893:25c8:1946

; CNAME records
blog    IN  CNAME www.example.com.
ftp     IN  CNAME www.example.com.

; MX records
@       IN  MX    10 mail.example.com.
@       IN  MX    20 mail2.example.com.

; TXT records
@       IN  TXT   "v=spf1 mx -all"

; SRV records
_sip._tcp  IN  SRV  10 60 5060 sip.example.com.
```

---

## DNS Load Balancing Techniques

### Round-Robin DNS
Multiple A records for the same name -- clients get different IPs in rotation:
```bash
$ dig loadbalanced.example.com A
;; ANSWER SECTION:
loadbalanced.example.com. 300 IN A 10.0.0.1
loadbalanced.example.com. 300 IN A 10.0.0.2
loadbalanced.example.com. 300 IN A 10.0.0.3
```

### GeoDNS
Returns different IP addresses based on the client's geographic location:

---

## DNS Load Balancing Techniques

![geodns](svg/courses/networking/networking-basics/04_dns_deep_dive/geodns.svg)

---

## DNS Load Balancing Techniques

Used by CDNs (Cloudflare, AWS Route53, Akamai) for latency-based routing.

---

## DNS Security Threats

| Attack | Description | Mitigation |
|--------|-------------|------------|
| DNS Spoofing | Forged responses redirect users | DNSSEC |
| DNS Cache Poisoning | Corrupt resolver cache | DNSSEC, source port randomization |
| DNS Amplification DDoS | Abuse open resolvers for DDoS | Rate limiting, response rate limiting |
| DNS Tunneling | Exfiltrate data over DNS queries | Monitor unusual query patterns |
| Domain Hijacking | Unauthorized domain transfer | Registrar lock, 2FA |
| Typosquatting | Register similar domain names | Monitor similar domains |

```bash
# Check for open resolver (should NOT respond to external queries)
$ dig @your-server example.com

# Test if DNS responses are being modified (MITM detection)
$ dig +dnssec example.com @8.8.8.8
$ dig +dnssec example.com @1.1.1.1
# Compare RRSIG values -- they should match
```

---

## Python DNS Example

```python
#!/usr/bin/env python
"""Simple DNS resolver using the socket library and dnspython."""

import socket

# Basic resolution using socket
def resolve_basic(hostname):
    """Resolve hostname using OS resolver."""
    try:
        ip = socket.gethostbyname(hostname)
        print(f"{hostname} -> {ip}")

        # Get all addresses (IPv4)
        ips = socket.gethostbyname_ex(hostname)
        print(f"  Hostname: {ips[0]}")
        print(f"  Aliases:  {ips[1]}")
        print(f"  IPs:      {ips[2]}")

        # Get address info (IPv4 + IPv6)
        results = socket.getaddrinfo(hostname, 80)
        for family, socktype, proto, canonname, sockaddr in results:
            print(f"  {socket.AddressFamily(family).name}: {sockaddr[0]}")
    except socket.gaierror as e:
        print(f"DNS resolution failed: {e}")

resolve_basic("example.com")
```

```bash
$ python dns_resolver.py
example.com -> 93.184.216.34
  Hostname: example.com
  Aliases:  []
  IPs:      ['93.184.216.34']
  AF_INET: 93.184.216.34
  AF_INET6: 2606:2800:220:1:248:1893:25c8:1946
```

---

## Advanced DNS with dnspython

```python
#!/usr/bin/env python
"""Advanced DNS queries using dnspython library."""

import dns.resolver
import dns.reversename
import dns.zone

def query_all_records(domain):
    """Query multiple record types for a domain."""
    record_types = ['A', 'AAAA', 'MX', 'NS', 'TXT', 'SOA']

    for rtype in record_types:
        try:
            answers = dns.resolver.resolve(domain, rtype)
            print(f"\n{rtype} records for {domain}:")
            for rdata in answers:
                print(f"  {rdata}")
                if rtype == 'A' or rtype == 'AAAA':
                    print(f"    TTL: {answers.rrset.ttl}s")
        except dns.resolver.NoAnswer:
            print(f"\n{rtype}: No records found")
        except dns.resolver.NXDOMAIN:
            print(f"\nDomain {domain} does not exist")
            return

def reverse_lookup(ip_address):
    """Perform reverse DNS lookup."""
    rev_name = dns.reversename.from_address(ip_address)
    try:
        answers = dns.resolver.resolve(rev_name, 'PTR')
        for rdata in answers:
            print(f"{ip_address} -> {rdata}")
    except dns.resolver.NXDOMAIN:
        print(f"No PTR record for {ip_address}")

query_all_records("google.com")
reverse_lookup("8.8.8.8")
```

---

## /etc/hosts File

The `/etc/hosts` file provides local static name resolution, checked before DNS.

```bash
$ cat /etc/hosts

# Standard entries
127.0.0.1       localhost
127.0.1.1       myhostname
::1             localhost ip6-localhost ip6-loopback

# Custom entries (useful for development)
192.168.1.100   myserver.local myserver
10.0.0.50       database.dev db
127.0.0.1       myapp.test

# Block unwanted domains (ad blocking)
0.0.0.0         ads.example.com
0.0.0.0         tracking.example.com
```

**Use cases:**
- Local development (map custom domains to localhost)
- Testing before DNS changes go live
- Simple ad/tracker blocking
- Override DNS for specific hosts

---

## DNS Best Practices

1. **Use at least two authoritative name servers** on different networks for redundancy
1. **Set appropriate TTLs**: lower before changes, raise after stability confirmed
1. **Implement DNSSEC** for zones you control
1. **Monitor DNS health**: check resolution times, error rates
1. **Use DNS over HTTPS/TLS** for privacy on client devices
1. **Keep SOA serial numbers** updated when changing zone files
1. **Avoid CNAME at zone apex** -- use A/AAAA records instead
1. **Document your DNS records** -- maintain a record of what each entry is for
1. **Use separate DNS providers** for redundancy (multi-provider DNS)
1. **Test changes** with `dig` before and after applying them

```bash
# Quick DNS health check script
#!/bin/bash
DOMAIN="example.com"
RESOLVERS="8.8.8.8 1.1.1.1 9.9.9.9"

echo "DNS Health Check for $DOMAIN"
echo "=============================="
for resolver in $RESOLVERS; do
    result=$(dig @$resolver $DOMAIN +short +time=2)
    time=$(dig @$resolver $DOMAIN +stats | grep "Query time" | awk '{print $4}')
    echo "Resolver $resolver: $result (${time}ms)"
done
```

---

## Review: DNS Key Concepts

- DNS is a distributed hierarchical naming system
- Multiple record types serve different purposes (A, AAAA, CNAME, MX, TXT, NS, SOA, PTR, SRV)
- Resolution involves recursive resolvers and iterative queries through the hierarchy
- TTL controls caching duration -- balance between freshness and performance
- DNSSEC adds cryptographic authentication to prevent spoofing
- DoH and DoT encrypt DNS queries for privacy
- Tools: `dig`, `nslookup`, `host`, `resolvectl` for querying and debugging
- `/etc/resolv.conf` and `/etc/hosts` are key configuration files on Linux
