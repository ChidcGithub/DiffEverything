# OpenSSH public key fixtures

`test_openssh_pub_key1.pub` (DSA/1024, comment `Test1`) and
`test_openssh_pub_key2.pub` (RSA/4096, comment `Test2`) are copied from the
diffoscope test suite:

- https://github.com/diffoscope/diffoscope/tree/master/tests/data

The comparator decodes the base64 wire blob and exposes individual key
parameters so a key-type/parameter change is reported semantically.
