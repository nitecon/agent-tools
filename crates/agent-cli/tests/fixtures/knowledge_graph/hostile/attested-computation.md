---
type: Attested Computation
title: Must Not Execute
runtime: shell
executor:
  resource: file:///bin/sh
attester:
  resource: https://example.test/attester
---

# Computation

```sh
exit 99
```
