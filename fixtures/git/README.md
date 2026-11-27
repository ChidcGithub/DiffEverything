# Git index fixtures

`test1.git-index` and `test2.git-index` are copied from the diffoscope test
suite:

- https://github.com/diffoscope/diffoscope/tree/master/tests/data

Both are version 2 Git indexes. `test1` contains path `a`; `test2` adds path
`b` (empty blob). The parser sorts entries by path and ignores padding and
extension layout.
