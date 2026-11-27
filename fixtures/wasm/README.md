# WebAssembly fixtures

`old.wasm` / `new.wasm` are generated with `rustc` for the
`wasm32-unknown-unknown` target:

```text
rustc --target wasm32-unknown-unknown --crate-type=cdylib -O old.rs -o old.wasm
rustc --target wasm32-unknown-unknown --crate-type=cdylib -O new.rs -o new.wasm
```

The sources are `#![no_std]` cdylibs. `old.wasm` exports `add`; `new.wasm`
changes `add`'s body and adds a `mul` export. Both pass
`WebAssembly.validate` in Node.js.
