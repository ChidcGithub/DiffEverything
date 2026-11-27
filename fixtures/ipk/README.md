# IPK fixtures

`old.ipk` and `new.ipk` are OpenWrt packages copied from the diffoscope test
suite:

- https://github.com/diffoscope/diffoscope/tree/master/tests/data

Each is a gzip-compressed tar containing `debian-binary`, `control.tar.gz` and
`data.tar.gz`. `control` differs in version and installed size; the data
payload is compared recursively.
