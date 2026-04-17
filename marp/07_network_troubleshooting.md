---
tags:
  - networking:troubleshooting
  - networking:tools
level: beginner
category: networking
audience:
  - audiences:developers
  - audiences:devops
  - audiences:sysadmins

---
# Network Troubleshooting
## Tools and Techniques for Diagnosing Network Issues

---

## Troubleshooting Methodology

1. **Identify** the problem: what exactly fails? For whom? Since when?
1. **Isolate** the layer: work bottom-up (physical → link → network → transport → app)
1. **Test** one variable at a time: `ping` gateway, then DNS, then the target
1. **Compare** working vs broken: `diff` configs, check recent changes
1. **Document** findings: timestamps, commands run, outputs observed

- Most network problems are DNS, firewall rules, or routing

---

## Troubleshooting Methodology

![troubleshooting_methodology](svg/courses/networking/networking-basics/07_network_troubleshooting/troubleshooting_methodology.svg)

---

## Layer-by-Layer Troubleshooting

Work from the bottom of the stack upward:

| Layer | Check | Tools |
|-------|-------|-------|
| Physical | Cable connected? Link light on? | `ip link`, `ethtool` |
| Data Link | Interface up? MAC address? | `ip link`, `arp`, `bridge` |
| Network | IP assigned? Route exists? | `ip addr`, `ip route`, `ping` |
| Transport | Port open? Connection established? | `ss`, `telnet`, `nc` |
| DNS | Name resolves? | `dig`, `nslookup`, `host` |
| Application | Service running? Correct response? | `curl`, `wget`, logs |

```bash
# Quick bottom-up diagnostic sequence
$ ip link show eth0          # Is the interface up?
$ ip addr show eth0          # Do we have an IP?
$ ip route                   # Do we have a default route?
$ ping -c 3 gateway-ip      # Can we reach the gateway?
$ ping -c 3 8.8.8.8         # Can we reach the internet?
$ dig example.com            # Does DNS work?
$ curl -I https://example.com # Does the application work?
```

---

## The ip Command

The `ip` command is the modern replacement for `ifconfig`, `route`, and `arp`.

```bash
# Show all interfaces and their addresses
$ ip addr show
1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536
    inet 127.0.0.1/8 scope host lo
2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500
    link/ether 52:54:00:12:34:56 brd ff:ff:ff:ff:ff:ff
    inet 10.0.0.5/24 brd 10.0.0.255 scope global eth0
    inet6 fe80::5054:ff:fe12:3456/64 scope link

# Show only one interface
$ ip addr show dev eth0

# Show interface statistics (packets, errors, drops)
$ ip -s link show eth0
2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500
    RX:  bytes  packets  errors  dropped
         1234567 8901    0       0
    TX:  bytes  packets  errors  dropped
         7654321 5432    0       0

# Bring interface up/down
$ sudo ip link set eth0 up
$ sudo ip link set eth0 down

# Add/remove IP address
$ sudo ip addr add 10.0.0.100/24 dev eth0
$ sudo ip addr del 10.0.0.100/24 dev eth0
```

---

## Routing Table

```bash
# Show routing table
$ ip route
default via 10.0.0.1 dev eth0 proto dhcp metric 100
10.0.0.0/24 dev eth0 proto kernel scope link src 10.0.0.5
172.17.0.0/16 dev docker0 proto kernel scope link src 172.17.0.1

# Explanation of the default route:
# default          → matches any destination not in other rules
# via 10.0.0.1     → send to this gateway
# dev eth0         → through this interface
# proto dhcp       → learned via DHCP
# metric 100       → route priority (lower = preferred)

# Show route for a specific destination
$ ip route get 8.8.8.8
8.8.8.8 via 10.0.0.1 dev eth0 src 10.0.0.5 uid 1000

# Add a static route
$ sudo ip route add 192.168.2.0/24 via 10.0.0.1 dev eth0

# Delete a route
$ sudo ip route del 192.168.2.0/24

# Show routing cache (not all kernels support this)
$ ip route show cache
```

---

## ping: Basic Connectivity Test

`ping` sends ICMP Echo Request packets and measures round-trip time.

```bash
# Basic ping
$ ping -c 5 8.8.8.8
PING 8.8.8.8 (8.8.8.8) 56(84) bytes of data.
64 bytes from 8.8.8.8: icmp_seq=1 ttl=118 time=5.42 ms
64 bytes from 8.8.8.8: icmp_seq=2 ttl=118 time=5.38 ms
64 bytes from 8.8.8.8: icmp_seq=3 ttl=118 time=5.51 ms
64 bytes from 8.8.8.8: icmp_seq=4 ttl=118 time=5.44 ms
64 bytes from 8.8.8.8: icmp_seq=5 ttl=118 time=5.39 ms

--- 8.8.8.8 ping statistics ---
5 packets transmitted, 5 received, 0% packet loss, time 4006ms
rtt min/avg/max/mdev = 5.380/5.428/5.510/0.044 ms

# Ping with specific packet size
$ ping -c 3 -s 1472 8.8.8.8        # Test MTU (1472 + 28 = 1500)

# Ping with Do Not Fragment flag
$ ping -c 3 -s 1472 -M do 8.8.8.8  # Path MTU discovery

# Ping flood (requires root, careful!)
$ sudo ping -f -c 1000 10.0.0.1

# Ping with specific interface
$ ping -c 3 -I eth0 8.8.8.8

# Ping with timestamp
$ ping -c 3 -D 8.8.8.8
```

**Interpreting ping output:**
- **ttl**: Time To Live -- decremented by each router. Helps estimate hop count.
- **time**: Round-trip time in milliseconds
- **packet loss**: % of packets that didn't get a response

---

## What Ping Failures Mean

![what_ping_failures_mean](svg/courses/networking/networking-basics/07_network_troubleshooting/what_ping_failures_mean.svg)

---

## Understanding Packet Loss

- **0% loss, low latency**: healthy connection
- **0% loss, high latency**: congestion or long path (check `mtr` hop-by-hop)
- **Intermittent loss (1-5%)**: congestion, flaky link, or Wi-Fi interference
- **100% loss**: host down, firewall blocking, or routing problem
- **Loss at a specific hop in mtr**: that router deprioritizes ICMP (often harmless)
- **Loss increasing from a hop onward**: real problem at that hop

Key: always test from **both ends** — the problem may be asymmetric

---

## traceroute: Path Discovery

`traceroute` shows each hop (router) between you and the destination.

```bash
# Basic traceroute
$ traceroute 8.8.8.8
traceroute to 8.8.8.8 (8.8.8.8), 30 hops max, 60 byte packets
 1  gateway (10.0.0.1)  0.456 ms  0.412 ms  0.378 ms
 2  isp-router (203.0.113.1)  2.123 ms  2.098 ms  2.067 ms
 3  core-router (198.51.100.1)  5.234 ms  5.201 ms  5.178 ms
 4  * * *    ← Router doesn't respond to traceroute probes
 5  google-peer (72.14.236.1)  10.567 ms  10.534 ms  10.512 ms
 6  dns.google (8.8.8.8)  10.890 ms  10.856 ms  10.823 ms

# Use ICMP instead of UDP (may get through more firewalls)
$ sudo traceroute -I 8.8.8.8

# Use TCP SYN (most reliable through firewalls)
$ sudo traceroute -T -p 443 8.8.8.8

# Show AS numbers (useful for identifying networks)
$ traceroute -A 8.8.8.8
```

**How traceroute works:**

```misc
TTL=1: Packet reaches first router → router sends ICMP Time Exceeded
TTL=2: Packet reaches second router → router sends ICMP Time Exceeded
TTL=3: Packet reaches third router → router sends ICMP Time Exceeded
...
TTL=N: Packet reaches destination → destination sends reply
```

---

## mtr: Combined ping + traceroute

`mtr` (My Traceroute) continuously runs traceroute and calculates statistics over time.

```bash
# Interactive mode (default)
$ mtr 8.8.8.8

# Report mode (run for 100 cycles, then print report)
$ mtr -r -c 100 8.8.8.8

                        Host                 Loss%  Snt   Last  Avg   Best  Wrst  StDev
  1. gateway (10.0.0.1)                       0.0%  100   0.4   0.5   0.3   1.2   0.1
  2. isp-router (203.0.113.1)                 0.0%  100   2.1   2.3   1.8   4.5   0.3
  3. core-router (198.51.100.1)               0.0%  100   5.2   5.4   4.8   8.1   0.5
  4. ???                                     100.0%  100   0.0   0.0   0.0   0.0   0.0
  5. google-peer (72.14.236.1)                0.0%  100  10.5  10.7  10.1  15.2   0.8
  6. dns.google (8.8.8.8)                     0.0%  100  10.8  11.0  10.4  16.1   0.9

# Key columns:
# Loss%  → Packet loss at each hop
# Snt    → Packets sent
# Last   → Last RTT (ms)
# Avg    → Average RTT (ms)
# Best   → Best (lowest) RTT
# Wrst   → Worst (highest) RTT
# StDev  → Standard deviation (jitter)

# Wide report (show full hostnames)
$ mtr -r -w -c 100 8.8.8.8

# Use TCP mode
$ mtr -T -P 443 8.8.8.8

# JSON output (for automation)
$ mtr -j -c 10 8.8.8.8
```

---

## Interpreting mtr Results

![interpreting_mtr_results](svg/courses/networking/networking-basics/07_network_troubleshooting/interpreting_mtr_results.svg)

---

## tcpdump: Packet Capture

`tcpdump` captures and displays network packets. Essential for deep troubleshooting.

```bash
# Capture all traffic on eth0
$ sudo tcpdump -i eth0

# Capture with human-readable output
$ sudo tcpdump -i eth0 -nn    # Don't resolve names
$ sudo tcpdump -i eth0 -nnv   # Verbose
$ sudo tcpdump -i eth0 -nnvv  # Very verbose

# Filter by host
$ sudo tcpdump -i eth0 host 10.0.0.5
$ sudo tcpdump -i eth0 src host 10.0.0.5
$ sudo tcpdump -i eth0 dst host 10.0.0.5

# Filter by port
$ sudo tcpdump -i eth0 port 80
$ sudo tcpdump -i eth0 port 443 or port 80

# Filter by protocol
$ sudo tcpdump -i eth0 tcp
$ sudo tcpdump -i eth0 udp
$ sudo tcpdump -i eth0 icmp

# Capture to file for later analysis (in Wireshark)
$ sudo tcpdump -i eth0 -w capture.pcap

# Read from capture file
$ tcpdump -r capture.pcap

# Limit capture to N packets
$ sudo tcpdump -i eth0 -c 100

# Show packet contents in hex and ASCII
$ sudo tcpdump -i eth0 -XX port 80
```

---

## tcpdump Filter Examples

```bash
# HTTP traffic (request/response)
$ sudo tcpdump -i eth0 -nn 'tcp port 80'

# DNS queries and responses
$ sudo tcpdump -i eth0 -nn 'udp port 53'

# TCP SYN packets only (new connections)
$ sudo tcpdump -i eth0 -nn 'tcp[tcpflags] & tcp-syn != 0'

# TCP RST packets (connection resets)
$ sudo tcpdump -i eth0 -nn 'tcp[tcpflags] & tcp-rst != 0'

# Packets to/from a subnet
$ sudo tcpdump -i eth0 -nn net 192.168.1.0/24

# Packets larger than 1000 bytes
$ sudo tcpdump -i eth0 -nn 'greater 1000'

# Exclude SSH traffic (when troubleshooting over SSH)
$ sudo tcpdump -i eth0 -nn 'not port 22'

# Complex filter: HTTP traffic to specific host
$ sudo tcpdump -i eth0 -nn 'tcp port 80 and host 10.0.0.50'

# Watch TCP handshakes
$ sudo tcpdump -i eth0 -nn 'tcp[tcpflags] & (tcp-syn|tcp-fin) != 0'
```

---

## Reading tcpdump Output

```console
$ sudo tcpdump -i eth0 -nn port 80

15:30:01.123456 IP 10.0.0.5.54321 > 93.184.216.34.80: Flags [S], seq 1000, win 64240
15:30:01.145678 IP 93.184.216.34.80 > 10.0.0.5.54321: Flags [S.], seq 2000, ack 1001, win 65535
15:30:01.145700 IP 10.0.0.5.54321 > 93.184.216.34.80: Flags [.], ack 2001, win 64240
15:30:01.145800 IP 10.0.0.5.54321 > 93.184.216.34.80: Flags [P.], seq 1001:1201, ack 2001
15:30:01.167890 IP 93.184.216.34.80 > 10.0.0.5.54321: Flags [.], ack 1201, win 65535
15:30:01.168000 IP 93.184.216.34.80 > 10.0.0.5.54321: Flags [P.], seq 2001:3501, ack 1201
```

**TCP Flag meanings:**
| Flag | Symbol | Meaning |
|------|--------|---------|
| SYN | [S] | Connection initiation |
| SYN-ACK | [S.] | Connection acknowledgment |
| ACK | [.] | Acknowledgment |
| PSH-ACK | [P.] | Data push with ack |
| FIN | [F] | Connection termination |
| RST | [R] | Connection reset |

The sequence above shows: TCP handshake (SYN, SYN-ACK, ACK), then data exchange (PSH-ACK).

---

## Wireshark Basics

Wireshark provides a graphical interface for packet analysis, with powerful filtering and protocol decoding.

---

## Wireshark Basics

![wireshark_basics](svg/courses/networking/networking-basics/07_network_troubleshooting/wireshark_basics.svg)

---

## Wireshark Display Filters

Wireshark has its own filter syntax (different from tcpdump BPF):

```misc
# By IP address
ip.addr == 10.0.0.5
ip.src == 10.0.0.5
ip.dst == 93.184.216.34

# By port
tcp.port == 80
tcp.dstport == 443
udp.port == 53

# By protocol
http
dns
tls
tcp
arp

# TCP flags
tcp.flags.syn == 1
tcp.flags.reset == 1

# HTTP specific
http.request.method == "GET"
http.response.code == 200
http.host contains "example"

# Combine with logical operators
tcp.port == 80 && ip.addr == 10.0.0.5
tcp.port == 80 || tcp.port == 443
!(tcp.port == 22)

# Follow TCP stream: right-click a packet → Follow → TCP Stream
# Shows the complete conversation in readable form
```

---

## Useful Wireshark Features

![useful_wireshark_features](svg/courses/networking/networking-basics/07_network_troubleshooting/useful_wireshark_features.svg)

---

## Useful Wireshark Features

```bash
# Capture with tcpdump, analyze in Wireshark
$ sudo tcpdump -i eth0 -w /tmp/capture.pcap -c 10000 port 80
# Then open /tmp/capture.pcap in Wireshark
# Command-line Wireshark (tshark)
$ tshark -i eth0 -f "port 80" -Y "http.request" -T fields \
    -e ip.src -e http.host -e http.request.uri
```

---

## ARP Troubleshooting

ARP maps IP addresses to MAC addresses on the local network.

```bash
# View ARP cache
$ ip neighbor show
10.0.0.1 dev eth0 lladdr 52:54:00:aa:bb:cc REACHABLE
10.0.0.10 dev eth0 lladdr 52:54:00:dd:ee:ff STALE

# ARP states:
# REACHABLE → recently confirmed
# STALE     → not confirmed recently
# DELAY     → waiting for confirmation
# FAILED    → no response to ARP request

# Clear ARP cache for specific entry
$ sudo ip neighbor del 10.0.0.10 dev eth0

# Flush entire ARP cache
$ sudo ip neighbor flush all

# Watch ARP traffic
$ sudo tcpdump -i eth0 arp
ARP, Request who-has 10.0.0.1 tell 10.0.0.5, length 28
ARP, Reply 10.0.0.1 is-at 52:54:00:aa:bb:cc, length 28

# Send ARP request manually
$ arping -c 3 10.0.0.1
ARPING 10.0.0.1 from 10.0.0.5 eth0
Unicast reply from 10.0.0.1 [52:54:00:aa:bb:cc]  0.563ms
```

**Common ARP issues:**
- Duplicate IP addresses (two hosts with same IP)
- ARP cache poisoning (security attack)
- Stale ARP entries after MAC address change

---

## Common Network Issues: Diagnosis Guide

### Issue: "Cannot connect to remote service"

```bash
# Step 1: Is the interface up?
$ ip link show eth0 | grep "state UP"

# Step 2: Do we have an IP address?
$ ip addr show eth0 | grep "inet "

# Step 3: Can we reach the gateway?
$ ping -c 3 $(ip route | grep default | awk '{print $3}')

# Step 4: Can we reach the internet?
$ ping -c 3 8.8.8.8

# Step 5: Does DNS work?
$ dig example.com +short

# Step 6: Can we reach the specific host?
$ ping -c 3 target-host

# Step 7: Is the port open?
$ nc -zv target-host 443
$ ss -tn dst target-host

# Step 8: Check for firewall blocking
$ sudo iptables -L -n | grep -i drop
$ sudo nft list ruleset | grep drop
```

---

## Common Network Issues: MTU Problems

MTU (Maximum Transmission Unit) mismatches cause strange symptoms: some connections work, large transfers fail.

```bash
# Check interface MTU
$ ip link show eth0 | grep mtu
2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500

# Test Path MTU to a destination
$ ping -c 3 -s 1472 -M do 8.8.8.8
# -s 1472: payload size (1472 + 8 ICMP + 20 IP = 1500)
# -M do: Don't Fragment

# If this fails:
$ ping -c 3 -s 1472 -M do 8.8.8.8
ping: local error: message too long, mtu=1400
# The path MTU is 1400, not 1500

# Common MTU values:
# 1500 → Standard Ethernet
# 1400 → VPN tunnels, some cloud networks
# 9000 → Jumbo frames (data center)
# 1280 → Minimum for IPv6

# Fix: reduce MTU
$ sudo ip link set eth0 mtu 1400

# Or enable Path MTU Discovery
$ sudo sysctl -w net.ipv4.ip_no_pmtu_disc=0
```

---

## Common Network Issues: DNS Problems

```bash
# Symptom: ping by IP works, but not by name
$ ping 8.8.8.8           # Works
$ ping google.com         # Fails: "Name or service not known"

# Diagnosis:
# 1. Check DNS configuration
$ cat /etc/resolv.conf
$ resolvectl status

# 2. Test DNS resolution explicitly
$ dig google.com @8.8.8.8    # Query Google's DNS
$ dig google.com @1.1.1.1    # Query Cloudflare's DNS

# 3. If external DNS works but system DNS doesn't:
$ dig google.com @127.0.0.53  # Test local resolver (systemd-resolved)

# 4. Check /etc/nsswitch.conf
$ grep hosts /etc/nsswitch.conf
hosts: files dns     # files = /etc/hosts, dns = /etc/resolv.conf

# 5. Flush DNS cache
$ sudo resolvectl flush-caches

# 6. Common fix: set DNS manually in /etc/resolv.conf
nameserver 8.8.8.8
nameserver 1.1.1.1
```

---

## Common Network Issues: Connection Resets and Timeouts

```bash
# Symptom: Connection reset (RST)
# Possible causes:
# - No process listening on the port
# - Firewall sending RST
# - Application crash

# Check if anything is listening on the port
$ ss -tlnp | grep 8080
# Empty output → nothing listening

# Watch for RST packets
$ sudo tcpdump -i eth0 -nn 'tcp[tcpflags] & tcp-rst != 0'

# Symptom: Connection timeout
# Possible causes:
# - Firewall silently dropping packets (no RST)
# - Host unreachable (but no ICMP unreachable sent)
# - Network black hole

# Test with aggressive timeout
$ nc -zv -w 3 target-host 8080
# -w 3: 3-second timeout

# Check for firewall drops
$ sudo iptables -L -v -n | grep DROP
$ sudo dmesg | grep -i iptables
$ sudo journalctl -k | grep -i nft

# Check conntrack table (stateful firewall)
$ sudo conntrack -L | grep target-host
```

---

## Performance Troubleshooting

```bash
# Check interface errors and drops
$ ip -s link show eth0
# Look for: errors, dropped, overrun, carrier

# Check network latency distribution
$ ping -c 100 8.8.8.8 | tail -1
rtt min/avg/max/mdev = 5.1/5.4/8.2/0.4 ms

# Check for bufferbloat (latency increases under load)
# Terminal 1: generate load
$ iperf3 -c server-ip
# Terminal 2: measure latency during load
$ ping 8.8.8.8
# If latency jumps from 5ms to 200ms+ under load → bufferbloat

# Measure bandwidth
$ iperf3 -s                    # Start server
$ iperf3 -c server-ip          # Run client
$ iperf3 -c server-ip -R       # Reverse direction
$ iperf3 -c server-ip -u -b 100M  # UDP test at 100 Mbps

# Check for packet loss over time
$ mtr -r -c 200 target-host

# Monitor bandwidth usage per interface
$ sar -n DEV 1 5      # 5 samples, 1 second apart
$ nload eth0           # Real-time bandwidth graph
$ iftop -i eth0        # Per-connection bandwidth
```

---

## Troubleshooting Script

```bash
#!/bin/bash
# network_diagnostics.sh - Quick network health check

TARGET=${1:-8.8.8.8}
echo "=============================="
echo "Network Diagnostics Report"
echo "Target: $TARGET"
echo "Date: $(date)"
echo "=============================="

echo ""
echo "--- Interface Status ---"
ip -br link show

echo ""
echo "--- IP Addresses ---"
ip -br addr show

echo ""
echo "--- Routing Table ---"
ip route

echo ""
echo "--- DNS Configuration ---"
cat /etc/resolv.conf | grep -v "^#"

echo ""
echo "--- Gateway Ping ---"
GW=$(ip route | grep default | awk '{print $3}')
ping -c 3 -W 2 "$GW" 2>&1 | tail -2

echo ""
echo "--- Internet Ping ($TARGET) ---"
ping -c 5 -W 2 "$TARGET" 2>&1 | tail -2

echo ""
echo "--- DNS Resolution ---"
dig google.com +short +time=2

echo ""
echo "--- Listening Services ---"
ss -tlnp 2>/dev/null | head -15

echo ""
echo "--- Active Connections ---"
ss -tnp 2>/dev/null | head -15

echo ""
echo "--- Interface Errors ---"
ip -s link show | grep -A2 "RX:\|TX:" | grep -v "^--$"

echo ""
echo "=============================="
echo "Diagnostics complete"
```

---

## ethtool: Physical Layer Diagnostics

```bash
# Show interface details
$ sudo ethtool eth0
Settings for eth0:
    Speed: 1000Mb/s
    Duplex: Full
    Auto-negotiation: on
    Link detected: yes

# Show interface statistics (errors, collisions)
$ sudo ethtool -S eth0
NIC statistics:
     rx_packets: 1234567
     tx_packets: 7654321
     rx_errors: 0
     tx_errors: 0
     rx_dropped: 12
     tx_dropped: 0
     rx_crc_errors: 0
     collisions: 0

# Show driver information
$ sudo ethtool -i eth0
driver: virtio_net
version: 1.0.0

# Test network cable (if supported)
$ sudo ethtool --test eth0
```

---

## Quick Reference: Troubleshooting Commands

| Problem | Command |
|---------|---------|
| Is interface up? | `ip link show eth0` |
| What is my IP? | `ip addr show` |
| What is my gateway? | `ip route \| grep default` |
| Can I reach gateway? | `ping -c 3 gateway-ip` |
| Can I reach internet? | `ping -c 3 8.8.8.8` |
| Does DNS work? | `dig example.com` |
| What is listening? | `ss -tlnp` |
| Is port open remotely? | `nc -zv host port` |
| Where do packets go? | `traceroute host` |
| Sustained test | `mtr -r -c 100 host` |
| Capture packets | `sudo tcpdump -i eth0 -nn` |
| Check ARP table | `ip neighbor show` |
| Interface errors? | `ip -s link show eth0` |
| Bandwidth test | `iperf3 -c server-ip` |
| Check MTU | `ping -s 1472 -M do host` |

---

## Review: Network Troubleshooting Key Concepts

- Use a systematic, layer-by-layer approach (physical up to application)
- **ip** command replaces ifconfig/route/arp -- learn it well
- **ping** tests basic connectivity; interpret results carefully (timeouts vs resets)
- **traceroute/mtr** reveal the network path and where problems occur
- **tcpdump** captures raw packets for deep analysis
- **Wireshark** provides graphical packet analysis with powerful filters
- **ss** shows socket state, replacing netstat
- Common issues: DNS misconfiguration, firewall drops, MTU mismatches, routing problems
- Always check the simple things first: is the cable plugged in? Is the interface up?
- Document your findings and solutions for future reference
