# Java class fixtures

`Test1.class` and `Test2.class` are copied from the diffoscope test suite:

- https://github.com/diffoscope/diffoscope/tree/master/tests/data

Both compile the same `Test` class; only `main` bytecode differs
(`bipush 42; ireturn` vs `iconst_m1; ireturn`), as confirmed with
`javap -p -c`.

`old.jar` / `new.jar` package those class files with a manifest; the ZIP
comparator recurses into `Test.class` and applies the Java class semantic
diff.
