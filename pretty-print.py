from typing import Any, Iterator
from gdb import Frame
import gdb.printing
from gdb.FrameDecorator import FrameDecorator
import re
from os import environ

def delegateChildren(val: gdb.Value):
    try:
        for field in val.type.fields():
            try:
                yield field.name, val[field]
            except:
                yield field.name, '<Error>'
    except TypeError:
        return

def delegateToString(val: gdb.Value):
    try:
        val.type.fields()
        return None
    except:
        return val

class NonNull:
    def __init__(self, val: gdb.Value):
        pointer = val['pointer']

        if pointer.type.code == gdb.TYPE_CODE_PTR:
            self.pointer = pointer
            self.length = None
            self.type = pointer.type.target().name
        else:
            fields = pointer.type.fields()
            self.pointer, self.length = pointer[fields[0]], pointer[fields[1]]
            self.type = '[%s]' % self.pointer['data'].type

    def is_slice(self):
        return self.length is not None

class NonNullPrinter:
    def __init__(self, val: gdb.Value):
        self.ptr = NonNull(val)

    def to_string(self):
        return 'NonNull<%s>(0x%x)' % (self.ptr.type, self.ptr.pointer)

    def children(self):
        try:
            for item in delegateChildren(self.ptr.pointer.dereference()):
                yield item
        except:
            yield '[error]', '<Error>'

class ThreadLocalPrinter:
    def tls_base(self):
        return gdb.parse_and_eval("$gs_base")

    def __init__(self, val: gdb.Value):
        name = val.type.unqualified().strip_typedefs().name
        symbol, _ = gdb.lookup_symbol(name.replace('_access_', '_percpu_inner_'))
        value = symbol.value()
        value_type = value.type

        self.address = self.tls_base() + value.address.cast(gdb.lookup_type('size_t'))
        self.address = self.address.cast(value_type.pointer())

    def to_string(self):
        return delegateToString(self.address.dereference())

    def children(self):
        yield '[data]', self.address.dereference()

class LockPrinter:
    def __init__(self, val: gdb.Value):
        self.val = val

    def to_string(self):
        return delegateToString(self.val['value']['value'])

    def children(self):
        return delegateChildren(self.val['value']['value'])

class PagePrinter:
    def PAGE_ARRAY():
        return 0xffffff8040000000

    def __init__(self, val: gdb.Value):
        self.val = val

    def display_hint(self):
        'string'

    def to_string(self):
        return 'Pages of order %d' % self.val['order']

class UnwrapPrinter:
    def __init__(self, val: gdb.Value, name: str):
        self.val = val
        self.name = name

    def to_string(self):
        return delegateToString(self.val[self.name])

    def children(self):
        return delegateChildren(self.val[self.name])

class DentryPrinter:
    def __init__(self, val: gdb.Value):
        self.val = val

    def to_string(self):
        try:
            return 'Dentry'
        except:
            return '<Error>'

    def children(self):
        yield 'hash', self.val['hash']
        yield 'name', self.val['name']

class ArcPrinter:
    def __init__(self, valobj: gdb.Value):
        self._valobj = valobj
        self._ptr = NonNull(valobj['ptr'])
        for field in self._ptr.pointer.type.fields():
            print(field.name, self._ptr.pointer[field])
        self._value = self._ptr.pointer['data']
        self._strong = self._ptr.pointer['strong']['v']['value']
        self._weak = self._ptr.pointer['weak']['v']['value'] - 1

    def to_string(self):
        if self._ptr.type == '[u8]':
            return 'Arc(%s)' % self._value.address.lazy_string(encoding='utf-8', length=self._ptr.length)
        else:
            return 'Arc(strong={}, weak={})'.format(int(self._strong), int(self._weak))

    def children(self):
        if not self._ptr.is_slice():
            for item in delegateChildren(self._value):
                yield item
        else:
            iter = (self._value.address + idx for idx in range(self._ptr.length))
            for idx, ptr in enumerate(iter):
                elem = ptr.dereference()
                try:
                    str(elem)
                    yield str(idx), elem
                except RuntimeError:
                    yield str(idx), '[inaccessible]'
                    break

def build_pretty_printer(val: gdb.Value):
    type: gdb.Type = val.type.unqualified().strip_typedefs()
    typename = type.tag

    if typename == None:
        return None

    if re.compile(r"^.*::_access_[a-zA-Z0-9_]*$").match(typename):
        return ThreadLocalPrinter(val)

    if re.compile(r"^gbos_rust_part::sync::lock::Lock<.*>$").match(typename):
        return LockPrinter(val)

    if re.compile(r"^gbos_rust_part::kernel::mem::paging::Page$").match(typename):
        return PagePrinter(val)

    if re.compile(r"^gbos_rust_part::kernel::([a-zA-Z_]+::)*Dentry$").match(typename):
        return DentryPrinter(val)

    if re.compile(r"^(core::([a-z_]+::)+)UnsafeCell<.+>$").match(typename):
        return UnwrapPrinter(val, 'value')

    if re.compile(r"^(core::([a-z_]+::)+)NonNull<.+>$").match(typename):
        return NonNullPrinter(val)

    if re.compile(r"^(alloc::([a-z_]+::)+)Arc<.+>$").match(typename):
        return ArcPrinter(val)

    return None

gdb.execute('skip -rfu ^core::([a-zA-Z0-9_]+::)*[a-zA-Z0-9_<>]+')
gdb.execute('skip -rfu ^alloc::([a-zA-Z0-9_]+::)*[a-zA-Z0-9_<>]+')
gdb.execute('skip -rfu ^std::([a-zA-Z0-9_]+::)*[a-zA-Z0-9_<>]+')
gdb.execute('skip -rfu "^gbos_rust_part::sync::lock::Lock<[a-zA-Z0-9_<>: ,]+, [a-zA-Z0-9_<>: ,]+>::new<[a-zA-Z0-9_<>: ,]+, [a-zA-Z0-9_<>: ,]+>"')
gdb.execute('skip -rfu "^gbos_rust_part::sync::locked::Locked<[a-zA-Z0-9_<>: ,]+, [a-zA-Z0-9_<>: ,]+>::new<[a-zA-Z0-9_<>: ,]+, [a-zA-Z0-9_<>: ,]+>"')
gdb.pretty_printers.append(build_pretty_printer)
