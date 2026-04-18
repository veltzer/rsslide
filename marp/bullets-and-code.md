---
title: Bullets and Code Mixed
theme: default
paginate: true
---

## Error Handling in Rust

- Use Result for recoverable errors
- Use panic! for unrecoverable errors
- The ? operator propagates errors

```rust
fn read_config(path: &str) -> Result<String, io::Error> {
    let content = fs::read_to_string(path)?;
    Ok(content)
}
```

---

## Python List Comprehensions

- Concise syntax for creating lists
- Can include filtering with if
- Faster than equivalent for loops

```python
squares = [x**2 for x in range(10)]
evens = [x for x in range(20) if x % 2 == 0]
pairs = [(x, y) for x in [1,2] for y in [3,4]]
```

---

## Bash Safety Flags

- -e exits on first error
- -u treats unset variables as errors
- -o pipefail catches pipe failures

```bash
#!/bin/bash -euo pipefail

readonly CONFIG_DIR="/etc/myapp"
readonly LOG_FILE="/var/log/myapp.log"

main() {
    echo "Starting deployment..."
    deploy "$CONFIG_DIR"
}
main "$@"
```
