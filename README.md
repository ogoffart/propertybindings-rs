This crate is work in progress.

The Idea is to have develop a QML-inspired macros in rust.

Behind the scene, this uses the QML scene graph. But there is
only one QQuickItem. All rust Item are just node in the scene
graphs.
(For some node such as text node, there is an hidden QQuickItem
because there is no public API to get a text node)
only the `items` module depends on Qt.

See the `example/plusminus.rs` which can simply be run with

```
cargo run --example plusminus
```



