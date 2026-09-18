---
paths:
  - "iem-mixer/iem-ui/**/*.rs"
---

# Leptos `view!` macro gotchas (iem-ui)

## Never put a bare comparison inline in a `view!` attribute or `when=`

The `view!` macro tokenizes `>` / `>=` / `<` as tag boundaries. An inline
comparison such as

```rust
<Show when=move || snapshots.get().len() >= 50>
```

does not fail to parse — it mis-tokenizes and surfaces as an unrelated
`E0308 mismatched types` deep in the expansion (hit on #206, `snapshot_modal.rs`).

Bind the predicate to a named closure first, then reference it:

```rust
let at_limit = move || snapshots.get().len() >= MAX_SNAPSHOTS;
view! { <Show when=at_limit> ... </Show> }
```

Same rule for `class:=`, `style:=` and any attribute expression — anything with
`<`, `>`, `>=`, `<=` goes into a `let` outside the macro.
