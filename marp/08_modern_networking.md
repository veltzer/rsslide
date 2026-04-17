---
tags:
  - networking:sdn
  - networking:containers
  - networking:kubernetes
level: beginner
category: networking
audience:
  - audiences:developers
  - audiences:devops

---
# Modern Networking
## SDN, Containers, Service Mesh, and Beyond

---

## The Evolution of Networking
![the_evolution_of_networking](svg/courses/networking/networking-basics/08_modern_networking/the_evolution_of_networking.svg)

---

## The Evolution of Networking

Key trends driving modern networking:
- **Cloud computing**: networks must be dynamic and programmable
- **Microservices**: thousands of services need to communicate
- **Containers**: ephemeral workloads with dynamic IPs
- **Edge computing**: distributed infrastructure worldwide
- **Security**: zero trust, encryption everywhere

---

## Software-Defined Networking (SDN)

SDN separates the **control plane** (decision-making) from the **data plane** (packet forwarding).

---

## Software-Defined Networking (SDN)

![software_defined_networking_sdn](svg/courses/networking/networking-basics/08_modern_networking/software_defined_networking_sdn.svg)

---

## Software-Defined Networking (SDN)

**Benefits of SDN:**
- Centralized network management and visibility
- Programmable via APIs
- Dynamic reconfiguration without touching devices
- Vendor-neutral (OpenFlow protocol)

---

## SDN Architecture
![sdn_architecture](svg/courses/networking/networking-basics/08_modern_networking/sdn_architecture.svg)

---

## SDN Architecture

**SDN controllers:**
| Controller | Language | Use Case |
|-----------|----------|----------|
| ONOS | Java | Service provider, large scale |
| OpenDaylight | Java | Enterprise, multi-vendor |
| Floodlight | Java | Research, education |
| Ryu | Python | Lightweight, prototyping |

---
## Network Virtualization
Network virtualization creates logical networks on top of physical infrastructure.
**Key technologies:**
| Technology | Description |
|-----------|-------------|
| VLAN | L2 segmentation within a switch (802.1Q) |
| VXLAN | L2 overlay over L3 (extends VLANs across data centers) |
| GRE | Generic Routing Encapsulation (tunneling) |
| GENEVE | Generic Network Virtualization Encapsulation |
| NVGRE | Network Virtualization using GRE |

---

## Network Virtualization

![network_virtualization](svg/courses/networking/networking-basics/08_modern_networking/network_virtualization.svg)

---

## Container Networking: Docker Basics

Docker provides several network drivers for container communication.

```bash
# List Docker networks
$ docker network ls
NETWORK ID     NAME      DRIVER    SCOPE
abc123def456   bridge    bridge    local
ghi789jkl012   host      host      local
mno345pqr678   none      null      local

# Inspect the default bridge network
$ docker network inspect bridge
```

**Docker network modes:**

| Mode | Description |
|------|-------------|
| bridge | Default. Containers get private IPs on a virtual bridge |
| host | Container uses host's network stack directly |
| none | No networking |
| overlay | Multi-host networking (Docker Swarm) |
| macvlan | Container gets its own MAC address on physical network |

---

## Docker Bridge Network

The default networking mode. Docker creates a virtual bridge (`docker0`) and assigns private IPs.

---

## Docker Bridge Network

![docker_bridge_network](svg/courses/networking/networking-basics/08_modern_networking/docker_bridge_network.svg)

---

## Docker Bridge Network

```bash
# Create a custom bridge network
$ docker network create --driver bridge --subnet 172.20.0.0/16 mynet
# Run containers on the custom network
$ docker run -d --name web --network mynet nginx
$ docker run -d --name api --network mynet myapi
# Containers on the same custom network can resolve each other by name
$ docker exec web ping api    # Works! Docker DNS resolves "api"
```

---

## Docker Overlay Network

Overlay networks enable communication between containers across multiple Docker hosts (Swarm).

---

## Docker Overlay Network

![docker_overlay_network](svg/courses/networking/networking-basics/08_modern_networking/docker_overlay_network.svg)

---

## Docker Overlay Network

```bash
# Initialize Docker Swarm
$ docker swarm init
# Create overlay network
$ docker network create --driver overlay --attachable myoverlay
# Deploy service on overlay
$ docker service create --name web --network myoverlay --replicas 3 nginx
```

---
## Kubernetes Networking Model
Kubernetes networking has these fundamental requirements:
1. Every Pod gets its own IP address
1. All Pods can communicate with each other without NAT
1. All Nodes can communicate with all Pods without NAT
1. The IP a Pod sees itself as is the same IP others see it as

---

## Kubernetes Networking Model

![kubernetes_networking_model](svg/courses/networking/networking-basics/08_modern_networking/kubernetes_networking_model.svg)

---

## Kubernetes CNI (Container Network Interface)

CNI plugins implement the networking for Kubernetes.

| CNI Plugin | Technology | Key Feature |
|-----------|-----------|-------------|
| Flannel | VXLAN, host-gw | Simple, easy to set up |
| Calico | BGP, eBPF | Network policies, high performance |
| Cilium | eBPF | Advanced security, observability |
| Weave Net | VXLAN | Encrypted overlay |
| AWS VPC CNI | Native AWS | Pods get real VPC IPs |

```bash
# Check which CNI is running
$ kubectl get pods -n kube-system | grep -E "calico|flannel|cilium|weave"
calico-node-abc12   1/1   Running   0   5d
calico-node-def34   1/1   Running   0   5d

# View Pod IPs
$ kubectl get pods -o wide
NAME     READY   STATUS    IP          NODE
web-1    1/1     Running   10.0.1.2    node-a
web-2    1/1     Running   10.0.2.3    node-b
api-1    1/1     Running   10.0.1.4    node-a

# Test Pod-to-Pod connectivity
$ kubectl exec web-1 -- ping -c 3 10.0.2.3
```

---

## Kubernetes Services

Services provide stable network endpoints for dynamic Pods.

---

## Kubernetes Services

![kubernetes_services](svg/courses/networking/networking-basics/08_modern_networking/kubernetes_services.svg)

---

## Kubernetes Services

**Service types:**
| Type | Scope | Use Case |
|------|-------|----------|
| ClusterIP | Internal only | Default, inter-service communication |
| NodePort | External via node IP:port | Development, simple exposure |
| LoadBalancer | External via cloud LB | Production external access |
| ExternalName | DNS alias | Map to external service |
```yaml
# Service definition
apiVersion: v1
kind: Service
metadata:
  name: web-service
spec:
  type: ClusterIP
  selector:
    app: web
  ports:
    - port: 80
      targetPort: 8080
```

---

## Kubernetes Ingress

Ingress provides HTTP/HTTPS routing to services, supporting virtual hosts and path-based routing.

---

## Kubernetes Ingress

![kubernetes_ingress](svg/courses/networking/networking-basics/08_modern_networking/kubernetes_ingress.svg)

---

## Kubernetes Ingress

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: app-ingress
spec:
  rules:
    - host: api.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: api-service
                port:
                  number: 80
    - host: www.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: web-service
                port:
                  number: 80
```

---
## Kubernetes Network Policies
Network Policies control traffic flow between Pods (like a firewall for Pods).
```yaml
# Allow only specific Pods to access the database
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: db-access-policy
spec:
  podSelector:
    matchLabels:
      app: database
  policyTypes:
    - Ingress
  ingress:
    - from:
        - podSelector:
            matchLabels:
              role: backend
      ports:
        - protocol: TCP
          port: 5432
```

---

## Kubernetes Network Policies

![allow_only_specific_pods_to_access_the_database](svg/courses/networking/networking-basics/08_modern_networking/allow_only_specific_pods_to_access_the_database.svg)

---

## Service Mesh Concepts

A service mesh is a dedicated infrastructure layer for service-to-service communication.

---

## Service Mesh Concepts

![service_mesh_concepts](svg/courses/networking/networking-basics/08_modern_networking/service_mesh_concepts.svg)

---

## Service Mesh Concepts

**Popular service meshes:**
| Mesh | Sidecar Proxy | Key Feature |
|------|---------------|-------------|
| Istio | Envoy | Feature-rich, complex |
| Linkerd | linkerd2-proxy | Lightweight, simple |
| Consul Connect | Envoy or built-in | Multi-platform |

---

## Service Mesh: What It Provides

![service_mesh_what_it_provides](svg/courses/networking/networking-basics/08_modern_networking/service_mesh_what_it_provides.svg)

---

## CDN Architecture

A CDN (Content Delivery Network) caches content at edge locations worldwide to reduce latency.

---

## CDN Architecture

![cdn_architecture](svg/courses/networking/networking-basics/08_modern_networking/cdn_architecture.svg)

---

## CDN Architecture

**How CDNs work:**
1. User requests `cdn.example.com/image.png`
1. DNS resolves to nearest edge server (GeoDNS / anycast)
1. If edge has cached copy: serve immediately (cache HIT)
1. If not: fetch from origin, cache locally, then serve (cache MISS)
1. Subsequent requests from that region are served from cache
**Major CDN providers:** Cloudflare, AWS CloudFront, Akamai, Fastly, Google Cloud CDN

---

## CDN: Cache Control

```bash
# HTTP headers that control CDN caching:

# Cache-Control header (most important)
Cache-Control: public, max-age=86400           # Cache for 1 day
Cache-Control: private, no-cache               # Don't cache (user-specific)
Cache-Control: public, s-maxage=3600           # CDN caches 1 hour
Cache-Control: no-store                        # Never cache

# Common caching strategy:
# Static assets (JS, CSS, images): long cache + versioned URLs
Cache-Control: public, max-age=31536000        # 1 year
# URL: /assets/app.abc123.js                   # Hash in filename

# HTML pages: short cache or no-cache
Cache-Control: public, max-age=60              # 1 minute
# Or: Cache-Control: no-cache                  # Always revalidate

# API responses: usually no-cache
Cache-Control: private, no-store
```

```bash
# Check CDN cache status
$ curl -I https://cdn.example.com/image.png
HTTP/2 200
x-cache: HIT                    # Served from CDN cache
cf-cache-status: HIT            # Cloudflare-specific
age: 3600                       # Seconds in cache
cache-control: public, max-age=86400

# Purge CDN cache (Cloudflare example)
$ curl -X POST "https://api.cloudflare.com/client/v4/zones/ZONE_ID/purge_cache" \
    -H "Authorization: Bearer TOKEN" \
    -H "Content-Type: application/json" \
    --data '{"purge_everything":true}'
```

---

## WebSocket Protocol

WebSocket provides full-duplex communication over a single TCP connection, unlike HTTP's request-response model.

---

## WebSocket Protocol

![websocket_protocol](svg/courses/networking/networking-basics/08_modern_networking/websocket_protocol.svg)

---

## WebSocket Protocol

**WebSocket handshake (HTTP upgrade):**
```http
Client → Server:
GET /chat HTTP/1.1
Host: example.com
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==
Sec-WebSocket-Version: 13
Server → Client:
HTTP/1.1 101 Switching Protocols
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=
# After this, both sides can send messages freely
```

---

## WebSocket: Python Example

```python
#!/usr/bin/env python
"""WebSocket server using the websockets library."""

import asyncio
import websockets
import json

connected_clients = set()

async def handler(websocket, path):
    """Handle WebSocket connections."""
    connected_clients.add(websocket)
    client_addr = websocket.remote_address
    print(f"Client connected: {client_addr}")

    try:
        async for message in websocket:
            print(f"Received from {client_addr}: {message}")

            # Broadcast to all connected clients
            data = json.dumps({
                "sender": str(client_addr),
                "message": message
            })
            # Send to all except sender
            others = connected_clients - {websocket}
            if others:
                await asyncio.gather(
                    *[client.send(data) for client in others]
                )
    except websockets.exceptions.ConnectionClosed:
        pass
    finally:
        connected_clients.remove(websocket)
        print(f"Client disconnected: {client_addr}")

async def main():
    async with websockets.serve(handler, "0.0.0.0", 8765):
        print("WebSocket server on ws://0.0.0.0:8765")
        await asyncio.Future()  # Run forever

asyncio.run(main())
```

```bash
# Test with websocat (CLI WebSocket client)
$ websocat ws://localhost:8765
Hello, World!
```

---

## gRPC: Modern RPC Framework

gRPC uses HTTP/2 for transport and Protocol Buffers for serialization. Much more efficient than REST/JSON for service-to-service communication.

---

## gRPC: Modern RPC Framework

![grpc_modern_rpc_framework](svg/courses/networking/networking-basics/08_modern_networking/grpc_modern_rpc_framework.svg)

---

## gRPC: Modern RPC Framework

**Protocol Buffers definition:**
```protobuf
// user.proto
syntax = "proto3";
service UserService {
    rpc GetUser (UserRequest) returns (UserResponse);
    rpc ListUsers (ListRequest) returns (stream UserResponse);  // Server streaming
    rpc CreateUsers (stream UserRequest) returns (Summary);     // Client streaming
    rpc Chat (stream Message) returns (stream Message);         // Bidirectional
}
message UserRequest {
    int32 id = 1;
}
message UserResponse {
    int32 id = 1;
    string name = 2;
    string email = 3;
}
```

---

## gRPC vs REST Comparison

| Feature | REST | gRPC |
|---------|------|------|
| Protocol | HTTP/1.1 or HTTP/2 | HTTP/2 only |
| Payload | JSON (text) | Protobuf (binary) |
| Contract | OpenAPI/Swagger (optional) | .proto files (required) |
| Streaming | Limited (SSE, WebSocket) | Native bidirectional |
| Code generation | Optional | Built-in |
| Browser support | Native | Requires gRPC-Web proxy |
| Performance | Good | Excellent (~10x faster) |
| Human readable | Yes | No (binary) |

**When to use gRPC:**
- Microservice-to-microservice communication
- High-throughput, low-latency requirements
- Streaming data (real-time updates, video, telemetry)
- Polyglot environments (code generation for 10+ languages)

**When to use REST:**
- Public-facing APIs (browser clients)
- Simple CRUD operations
- Human-readable debugging needed
- Broad tooling/ecosystem requirements

---

## HTTP/3 and QUIC

HTTP/3 replaces TCP with QUIC (built on UDP) for better performance.

---

## HTTP/3 and QUIC

![http_3_and_quic](svg/courses/networking/networking-basics/08_modern_networking/http_3_and_quic.svg)

---

## HTTP/3 and QUIC

**QUIC advantages:**
- **Faster connection establishment**: 1-RTT (or 0-RTT on reconnect)
- **No head-of-line blocking**: stream loss doesn't block other streams
- **Connection migration**: survives IP changes (Wi-Fi to cellular)
- **Built-in encryption**: TLS 1.3 is mandatory and integrated
```bash
# Check if a site supports HTTP/3
$ curl --http3 -I https://cloudflare.com
HTTP/3 200
# Check with curl verbose
$ curl -v --http3 https://cloudflare.com 2>&1 | grep "using HTTP"
* using HTTP/3
```

---

## eBPF: Programmable Networking in the Kernel

eBPF (extended Berkeley Packet Filter) allows running custom programs in the Linux kernel without modifying kernel source.

---

## eBPF: Programmable Networking in the Kernel

![ebpf_programmable_networking_in_the_kernel](svg/courses/networking/networking-basics/08_modern_networking/ebpf_programmable_networking_in_the_kernel.svg)

---

## eBPF: Programmable Networking in the Kernel

**eBPF in networking:**
- **Cilium**: Kubernetes CNI that uses eBPF for networking and security (replaces iptables)
- **XDP**: eXpress Data Path -- process packets before they reach the kernel network stack
- Enables: load balancing, firewalling, observability at kernel speed

---

## Infrastructure as Code: Network Configuration

Modern networking is managed through code, not manual CLI commands.

```yaml
# Terraform: AWS VPC and networking
resource "aws_vpc" "main" {
  cidr_block = "10.0.0.0/16"

  tags = {
    Name = "production-vpc"
  }
}

resource "aws_subnet" "public" {
  vpc_id            = aws_vpc.main.id
  cidr_block        = "10.0.1.0/24"
  availability_zone = "us-east-1a"

  tags = {
    Name = "public-subnet"
  }
}

resource "aws_security_group" "web" {
  vpc_id = aws_vpc.main.id

  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}
```

---

## DNS-Based Service Discovery

Modern architectures use DNS for dynamic service discovery.

---

## DNS-Based Service Discovery

![dns_based_service_discovery](svg/courses/networking/networking-basics/08_modern_networking/dns_based_service_discovery.svg)

---

## DNS-Based Service Discovery

```bash
# Consul DNS interface
$ dig @127.0.0.1 -p 8600 web.service.consul SRV
;; ANSWER SECTION:
web.service.consul. 0 IN SRV 1 1 8080 node1.node.consul.
web.service.consul. 0 IN SRV 1 1 8080 node2.node.consul.
# Kubernetes CoreDNS
$ dig @10.96.0.10 web-service.default.svc.cluster.local
;; ANSWER SECTION:
web-service.default.svc.cluster.local. 30 IN A 10.96.0.100
# Kubernetes DNS naming convention:
# <service>.<namespace>.svc.cluster.local
# <pod-ip-dashes>.<namespace>.pod.cluster.local
```

---

## Observability: Network Monitoring

Modern networks require comprehensive observability.

---

## Observability: Network Monitoring

![observability_network_monitoring](svg/courses/networking/networking-basics/08_modern_networking/observability_network_monitoring.svg)

---

## Observability: Network Monitoring

```yaml
# Prometheus blackbox_exporter: probe network endpoints
modules:
  http_2xx:
    prober: http
    timeout: 5s
    http:
      valid_http_versions: ["HTTP/1.1", "HTTP/2.0"]
      valid_status_codes: [200]
  tcp_connect:
    prober: tcp
    timeout: 5s
  icmp_check:
    prober: icmp
    timeout: 5s
```

---

## Review: Modern Networking Key Concepts

- **SDN** separates control plane from data plane for programmable networks
- **Network virtualization** (VXLAN, GENEVE) creates overlay networks
- **Docker networking**: bridge (single host), overlay (multi-host)
- **Kubernetes networking**: flat network model, every Pod gets an IP
    - **CNI plugins** (Calico, Cilium) implement the network
    - **Services** provide stable endpoints
    - **Ingress** handles HTTP routing
    - **Network Policies** act as Pod-level firewalls
- **Service mesh** (Istio, Linkerd) handles mTLS, retries, observability
- **CDNs** cache content at edge locations for low latency
- **WebSocket** enables real-time bidirectional communication
- **gRPC** offers high-performance service-to-service communication
- **HTTP/3 + QUIC** eliminates head-of-line blocking with UDP transport
- **eBPF** enables kernel-level network programmability
- Modern networking is managed as code (Terraform, Kubernetes YAML)
