# Filtered list

This example keeps filtering, selection, and pagination in application state
while using Nagi TUI's semantic nodes and retained focus routing

It combines a Core `TextInput` with `List::filter`, `List::paginate`, a list
viewport, `Paginator`, and `Help`. Filtered callbacks retain the original item
indices, so application state does not need a second identity scheme. The List
is one Tab stop and its `Length` viewport follows keyboard selection

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example filtered_list
```

Type in the first field, use Tab to move between the results and paginator,
and use arrow keys to navigate the focused widget. Escape or Control-C exits
