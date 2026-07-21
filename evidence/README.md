# External engine evidence

`manifest.json` records the immutable size, SHA-256, observed container magic and verified role of
each research sample. The binary books and golden PDF are not committed to this repository.

Verify a local evidence set without loading complete files into memory:

```sh
node evidence/verify-evidence.mjs /path/to/books /path/to/golden-output
```

A missing or mismatched item fails the command. An unsupported input is evidence for probing and
safe rejection only; its presence in the manifest does not claim that a decoder exists.

## Structural probe evidence

`probe-evidence.mjs` independently exercises the same bounded-window rules as the Rust
`container-probe` crate against the external samples. It writes only structural metadata; it does
not decode, render or export a book.

```sh
node evidence/probe-evidence.mjs ../upload evidence/probe-results.json
```

The committed result keeps `decoderAvailable=false` even for a structurally parsed BKC. Finding an
XRef is not proof that the protected header can be decoded.
