# Form validation

This example keeps every field and validation result in application state

It combines Core `TextInput` nodes with `Select`, `Checkbox`, `Calendar`,
`Button`, `Panel`, and `Help`. Validation runs after every controlled update,
and the submit button is disabled until all required fields are valid

Run it from the Rust repository root:

```sh
cargo run -p nagi-tui-widgets --example form_validation
```

Use Tab and Shift-Tab to move between controls. Text fields accept Unicode,
arrow keys navigate selectors and the calendar, and Enter or Space activates a
focused control. Escape or Control-C exits
