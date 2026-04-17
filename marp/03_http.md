---
tags:
  - networking:http
  - networking:protocols
level: beginner
category: networking
audience:
  - audiences:developers
  - audiences:devops

---
# HTTP Protocol: Evolution and Versions

## From 1.0 to 3.0

---

## What is HTTP

- HTTP: Hypertext Transfer Protocol
- Foundation of data exchange on the Web
- Client-server protocol
- Stateless, but not sessionless

---

## What is HTTP

![what_is_http](svg/courses/networking/networking-basics/03_http/what_is_http.svg)

---

## HTTP Methods

| Method | Purpose | Idempotent? | Body? |
|--------|---------|-------------|-------|
| GET | Retrieve a resource | Yes | No |
| POST | Submit data / create resource | No | Yes |
| PUT | Replace a resource entirely | Yes | Yes |
| PATCH | Partial update | No | Yes |
| DELETE | Remove a resource | Yes | No |
| HEAD | Like GET but headers only | Yes | No |
| OPTIONS | List supported methods (CORS preflight) | Yes | No |

---

## HTTP Status Codes

| Range | Category | Common Examples |
|-------|----------|-----------------|
| 1xx | Informational | 101 Switching Protocols |
| 2xx | Success | 200 OK, 201 Created, 204 No Content |
| 3xx | Redirection | 301 Moved Permanently, 304 Not Modified |
| 4xx | Client Error | 400 Bad Request, 401 Unauthorized, 403 Forbidden, 404 Not Found |
| 5xx | Server Error | 500 Internal Server Error, 502 Bad Gateway, 503 Service Unavailable |

- APIs use status codes to communicate success/failure semantics
- `curl -o /dev/null -w '%{http_code}' URL` shows just the status code

---

## HTTP Request/Response in Practice

```bash
$ curl -v http://example.com/
> GET / HTTP/1.1
> Host: example.com
> User-Agent: curl/8.5.0
> Accept: */*
>
< HTTP/1.1 200 OK
< Content-Type: text/html; charset=UTF-8
< Content-Length: 1256
< Cache-Control: max-age=604800
<
<!doctype html>...
```

- Lines starting with `>` are the **request** (sent by client)
- Lines starting with `<` are the **response** (returned by server)
- Headers carry metadata; the body follows the blank line

---

## Important HTTP Headers

| Header | Purpose | Example |
|--------|---------|---------|
| Content-Type | Body format | `application/json` |
| Authorization | Authentication | `Bearer eyJhbG...` |
| Cache-Control | Caching rules | `max-age=3600, public` |
| Cookie / Set-Cookie | Session state | `session_id=abc123` |
| Accept | Client's preferred formats | `text/html, application/json` |
| ETag / If-None-Match | Conditional requests | `"v1.2.3"` |
| Content-Encoding | Compression | `gzip` |

- HTTP is stateless — cookies and tokens maintain session state

---

## HTTP/1.0 (1996)

- First standardized version
- One request-response pair per TCP connection
- Headers introduced
- Methods: GET, HEAD, POST

---

## HTTP/1.0 (1996)

![http_1_0_1996](svg/courses/networking/networking-basics/03_http/http_1_0_1996.svg)

---

## HTTP/1.1 (1997)

- **Persistent connections**: reuse TCP connection for multiple requests
- **Pipelining**: send multiple requests without waiting for responses
    - Rarely used in practice — responses must arrive in order (HOL blocking)
- **Host header**: enables virtual hosting (multiple sites on one IP)
- New methods: PUT, DELETE, TRACE, OPTIONS
- Chunked transfer encoding (stream data without knowing size upfront)

---

## HTTP/1.1 (1997)

![http_1_1_1997](svg/courses/networking/networking-basics/03_http/http_1_1_1997.svg)

---

## HTTP/1.1 Improvements

- Reduced latency for multiple requests
- Better bandwidth utilization
- Introduced caching mechanisms
- Added compression (Content-Encoding)

---

## Head-of-Line Blocking

- **HTTP/1.1 problem**: requests are sequential on one connection
    - If request #1 is slow, requests #2 and #3 wait behind it
    - Browsers open 6 parallel connections as a workaround
- **HTTP/2 fix**: multiplexing — all requests share one connection as independent streams
    - But TCP itself still has HOL blocking (one lost packet stalls all streams)
- **HTTP/3 fix**: QUIC uses independent streams over UDP
    - A lost packet only stalls its own stream, not others

---

## HTTP/2 (2015)

- Binary protocol (not text-based)
- Multiplexing (multiple requests/responses over single connection)
- Header compression (HPACK)
- Server push
- Stream prioritization

---

## HTTP/2 (2015)

![http_2_2015](svg/courses/networking/networking-basics/03_http/http_2_2015.svg)

---

## HTTP/2 Server Push

![http_2_server_push](svg/courses/networking/networking-basics/03_http/http_2_server_push.svg)

---

## HTTP/3 (2022)

- Based on QUIC protocol (Quick UDP Internet Connections)
- Replaces TCP with UDP
- Improved performance on poor networks
- Reduced connection establishment time
- Better multiplexing without head-of-line blocking

---

## HTTP/3 (2022)

![http_3_2022](svg/courses/networking/networking-basics/03_http/http_3_2022.svg)

---

## HTTP/3 Connection Establishment

![http_3_connection_establishment](svg/courses/networking/networking-basics/03_http/http_3_connection_establishment.svg)

---

## Version Comparison
| Feature           | HTTP/1.0 | HTTP/1.1 | HTTP/2   | HTTP/3   |
|-------------------|----------|----------|----------|----------|
| Connections       | One-off  | Persistent | Multiplexed | Multiplexed |
| Compression       | No       | Yes      | HPACK    | QPACK    |
| Multiplexing      | No       | Limited  | Yes      | Yes      |
| Server Push       | No       | No       | Yes      | Yes      |
| HOL Blocking      | Yes      | Yes      | Reduced  | Eliminated |
| Transport Protocol| TCP      | TCP      | TCP      | UDP (QUIC) |

---
## Key Takeaways

1. HTTP has evolved to meet increasing web demands
1. Each version improved performance and capabilities
1. HTTP/2 and HTTP/3 focus on multiplexing and reducing latency
1. Modern websites benefit from using the latest HTTP version
1. Understanding HTTP versions helps in web optimization

---

## Final image of HTTP1.1

![final_image_of_http1_1](svg/courses/networking/networking-basics/03_http/final_image_of_http1_1.svg)
