# Suggestion popup

This example keeps query parsing and cancellable latest-result search in the
application while `SuggestionPopup` owns anchored placement and interaction

Run it from `nagi-rs` with

```sh
cargo run -p nagi-tui-widgets --example suggestion_popup
```

Type to replace the current candidate request, use Up or Down to select, press
Enter to accept, and press Escape to close the popup. Control-C exits
