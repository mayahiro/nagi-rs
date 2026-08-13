# Derived Help documents

This example validates a Command Graph once, visits every visible command in
definition-order preorder, and renders one page at a time. The application
owns page naming, output routing, and filesystem writes

Render CommonMark:

```sh
cargo run -p nagi-cli-document --example documentation -- markdown
```

Render section 1 man source:

```sh
cargo run -p nagi-cli-document --example documentation -- man
```

The example writes `---` between pages only to make the streaming output easy
to inspect. It does not form part of either generated document
