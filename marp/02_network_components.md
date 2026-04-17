---
tags:
  - networking:networking
  - networking:infrastructure
level: beginner
category: networking
audience:
  - audiences:developers
  - audiences:devops

---
# Network Components: Understanding the Differences

---

## Chapter Overview

- Load Balancers
- Proxies
- Reverse Proxies
- NAT (Network Address Translation)
- Other Related Terms

---

## Load Balancer

- Distributes incoming network traffic across multiple servers
- Ensures no single server becomes overwhelmed
- Improves application responsiveness and availability
- Can operate at Layer 4 (TCP/UDP) or Layer 7 (HTTP)
- Layer 7 can route based on URL path, headers, or cookies

---

## Load Balancer

![load_balancer](svg/courses/networking/networking-basics/02_network_components/load_balancer.svg)

---

## Load Balancing Algorithms

| Algorithm | How it Works | Best For |
|-----------|-------------|----------|
| Round Robin | Each server gets the next request | Equal-capacity servers |
| Least Connections | Send to server with fewest active connections | Varying request duration |
| IP Hash | Hash client IP to pick server | Session persistence |
| Weighted | Servers get traffic proportional to weight | Mixed-capacity servers |

- Health checks remove failed servers from the pool
- Sticky sessions: route same client to same server (cookies or IP)

---

## Proxy Server

- Acts as an intermediary between clients and servers
- Can provide anonymity, security, and caching
- Often used to bypass restrictions or improve performance

---

## Proxy Server

![proxy_server](svg/courses/networking/networking-basics/02_network_components/proxy_server.svg)

---

## Reverse Proxy

- Sits in front of web servers and forwards client requests
- Provides benefits like load balancing, SSL termination, and caching
- Often used to enhance security and performance of web applications

---

## Reverse Proxy

![reverse_proxy](svg/courses/networking/networking-basics/02_network_components/reverse_proxy.svg)

---

## Reverse Proxy: nginx Configuration

```nginx
upstream backend {
    server 10.0.0.1:8080;
    server 10.0.0.2:8080;
    server 10.0.0.3:8080;
}

server {
    listen 80;
    location / {
        proxy_pass http://backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

- nginx acts as reverse proxy + load balancer
- `proxy_set_header` forwards the real client IP to backends

---

## NAT (Network Address Translation)

- Modifies network address information in packet headers
- Allows multiple devices to share a single public IP address
- Enhances security by hiding internal network structure

---

## NAT (Network Address Translation)

![nat_network_address_translation](svg/courses/networking/networking-basics/02_network_components/nat_network_address_translation.svg)

---

## Other Related Terms

### Firewall
- Monitors and controls network traffic based on rules
- **Stateful**: tracks connection state (allows return traffic automatically)
- **Stateless**: evaluates each packet independently against rules
- Modern firewalls inspect application-layer content (Layer 7)

### VPN (Virtual Private Network)
- Creates an encrypted tunnel over a public network
- Remote workers appear to be on the office LAN
- Common protocols: WireGuard, OpenVPN, IPSec

---

## Comparison

| Term | Primary Function | Location | Direction |
|------|------------------|----------|-----------|
| Load Balancer | Distribute traffic | In front of servers | Inbound |
| Proxy | Intermediary for clients | Client side | Outbound |
| Reverse Proxy | Intermediary for servers | Server side | Inbound |
| NAT | IP address translation | Network boundary | Both |
| Firewall | Traffic filtering | Network boundary | Both |
| VPN | Secure remote access | Between networks | Both |

---

## Summary

- **Load Balancers** distribute traffic across servers
- **Proxies** act on behalf of clients
- **Reverse Proxies** act on behalf of servers
- **NAT** translates private to public IP addresses
- **Firewalls** filter network traffic
- **VPNs** provide secure remote network access

Each component plays a crucial role in modern network architecture!
