---
tags:
  - networking:tcp-ip
  - networking:protocols
level: beginner
category: networking
audience:
  - audiences:developers
  - audiences:devops

---
# TCP/IP Fundamentals
## Understanding the Internet's Core Protocols

---

## What is TCP/IP?
- Transmission Control Protocol/Internet Protocol
- Foundation of the internet and modern networking
- Suite of protocols that enable reliable data transmission
- Developed in the 1970s by DARPA

---

## The TCP/IP Protocol Stack
![the_tcp_ip_protocol_stack](svg/courses/networking/networking-basics/01_tcp_ip/the_tcp_ip_protocol_stack.svg)

---

## Protocol Stack Details
- Application Layer: HTTP, FTP, SMTP, DNS
- Transport Layer: TCP, UDP
- Internet Layer: IP, ICMP, ARP
- Network Access Layer: Ethernet, Wi-Fi

---

## IP Addressing
- Unique identifier for devices on a network
- IPv4: 32-bit address (e.g., 192.168.1.1)
- IPv6: 128-bit address (e.g., 2001:0db8:85a3:0000:0000:8a2e:0370:7334)
- Divided into network and host portions

---

## IP Address Classes (IPv4)
![ip_address_classes_ipv4](svg/courses/networking/networking-basics/01_tcp_ip/ip_address_classes_ipv4.svg)

---

## Subnetting
- Dividing networks into smaller networks
- Uses subnet masks to define network boundaries
- Enhances network management and security
- Example: 255.255.255.0 (/24 notation)

---

## IP Packet Structure

![ip_packet_structure](svg/courses/networking/networking-basics/01_tcp_ip/ip_packet_structure.svg)

---

## TCP vs UDP
### Key Differences

| TCP | UDP |
|-----|-----|
| Connection-oriented | Connectionless |
| Reliable | Best effort |
| Ordered delivery | No order guarantee |
| Flow control | No flow control |

---

## TCP Three-Way Handshake
![tcp_three_way_handshake](svg/courses/networking/networking-basics/01_tcp_ip/tcp_three_way_handshake.svg)

---

## Seeing the Handshake with tcpdump

```bash
# Capture TCP handshake to port 80
sudo tcpdump -i any -nn 'tcp port 80 and (tcp[tcpflags] & (tcp-syn|tcp-fin) != 0)'

# Output shows the three-way handshake:
# 10:23:01 IP 192.168.1.10.54321 > 93.184.216.34.80: Flags [S], seq 1000
# 10:23:01 IP 93.184.216.34.80 > 192.168.1.10.54321: Flags [S.], seq 2000, ack 1001
# 10:23:01 IP 192.168.1.10.54321 > 93.184.216.34.80: Flags [.], ack 2001
```

- `S` = SYN, `S.` = SYN+ACK, `.` = ACK
- The seq/ack numbers confirm the handshake mechanism

---

## TCP Flow Control
- Prevents overwhelming receivers
- Uses sliding window mechanism
- Window size adjusts dynamically
- Enables efficient data transfer

---

## TCP Connection States

| State | Meaning |
|-------|---------|
| LISTEN | Server waiting for connections |
| SYN_SENT | Client sent SYN, waiting for SYN-ACK |
| ESTABLISHED | Connection open, data flows |
| FIN_WAIT_1 | Sent FIN, waiting for ACK |
| TIME_WAIT | Waiting to ensure remote got final ACK (2×MSL) |
| CLOSE_WAIT | Received FIN, application has not closed yet |

- `TIME_WAIT` lasts ~60s — many in this state means high connection churn
- `CLOSE_WAIT` accumulating means a bug: the application isn't calling `close()`

---

## TCP Congestion Control

- **Slow Start**: exponentially increase sending rate from 1 segment
- **Congestion Avoidance**: linearly increase after reaching threshold
- **Fast Retransmit**: resend after 3 duplicate ACKs (don't wait for timeout)
- **Fast Recovery**: halve the window instead of restarting from 1
- Modern algorithms: CUBIC (Linux default), BBR (Google, measures bandwidth)

---

## UDP Characteristics
- Lightweight protocol
- No connection establishment
- No guarantee of delivery
- Ideal for real-time applications

---

## Common Applications

| Protocol | Port | Use Case |
|----------|------|----------|
| HTTP | 80 | Web browsing |
| HTTPS | 443 | Secure web |
| FTP | 21 | File transfer |
| DNS | 53 | Name resolution |

---

## Well-Known Port Numbers

- Ports 0-1023: well-known ports (require root/admin)
- Ports 1024-49151: registered ports (applications)
- Ports 49152-65535: dynamic/ephemeral ports (OS-assigned)
- A server **listens** on a port; a client connects **from** an ephemeral port
- `ss -tlnp` shows listening TCP ports on Linux

---

## Domain Name System (DNS)
![domain_name_system_dns](svg/courses/networking/networking-basics/01_tcp_ip/domain_name_system_dns.svg)

---

## ARP (Address Resolution Protocol)
- Maps IP addresses to MAC addresses
- Essential for local network communication
- Maintains ARP cache for efficiency
- Broadcast-based protocol

---

## ICMP (Internet Control Message Protocol)
- Network diagnostic tool
- Error reporting
- Echo request/reply (ping)
- Path MTU discovery

---

## Network Security Basics
- Firewalls
- Access Control Lists
- Encryption (IPSec)
- Network Address Translation (NAT)

---

## NAT Operation
![nat_operation](svg/courses/networking/networking-basics/01_tcp_ip/nat_operation.svg)

---

## IPv6 Features
- Larger address space
- Built-in security (IPSec)
- Simplified header format
- Better QoS support
- No need for NAT

---

## IPv6 Address Types
- Unicast
- Multicast
- Anycast
- Link-local
- Site-local

---

## Quality of Service (QoS)
- Traffic prioritization
- Bandwidth allocation
- Delay management
- Loss prevention
- Service guarantees

---

## Common Network Issues

- **Packet loss**: dropped packets cause retransmissions (TCP) or gaps (UDP)
- **Latency**: round-trip delay; affects interactive applications
- **Jitter**: variation in latency; critical for voice/video (causes choppy audio)
- **Congestion**: too much traffic for the link capacity; causes loss + latency
- **MTU mismatch**: oversized packets get fragmented or dropped
- Measure with: `ping` (latency/loss), `mtr` (per-hop), `iperf3` (throughput)

---

## Troubleshooting Tools
- ping
- traceroute/tracert
- nslookup/dig
- netstat
- Wireshark

---

## Best Practices
- Regular monitoring
- Security updates
- Documentation
- Redundancy
- Backup systems

---

## Future of TCP/IP
- IPv6 adoption
- QUIC protocol
- Network automation
- SDN integration
- Enhanced security

---

## Review & Key Takeaways
- TCP/IP is fundamental to networking
- Understanding layers helps troubleshooting
- Security is critical
- Protocol selection matters
- Continuous evolution
