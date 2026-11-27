# AR fixtures

`test1.a` and `test2.a` are copied from the diffoscope test suite:

- https://github.com/diffoscope/diffoscope/tree/master/tests/data

Both are GNU `ar` static libraries containing a different `test.o` ELF
object, so DiffEverything exercises AR recursion into the executable
comparator. sd-old.a / sd-new.a are BSD-format archives created with
llvm-ar rc --format=bsd, including long #1/ member names.
