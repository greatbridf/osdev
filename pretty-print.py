from typing import Any, Iterator
from gdb import Frame
import gdb.printing
from gdb.FrameDecorator import FrameDecorator
import re

def parseCompressedPairElement(elem: gdb.Value) -> gdb.Value:
    return elem[elem.type.fields()[0]]

def parseCompressedPair(cpair: gdb.Value) -> tuple[gdb.Value, gdb.Value]:
    fields = cpair.type.fields()

    first = cpair[fields[0]]
    second = cpair[fields[1]]

    return parseCompressedPairElement(first), parseCompressedPairElement(second)

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

class vectorPrinter:
    class _iterator:
        def __init__(self, item, end, idx):
            self.item = item
            self.end = end
            self.size = self.end - self.item
            self.idx = idx

        def __iter__(self):
            return self

        def __next__(self):
            if self.item >= self.end:
                raise StopIteration
            key = str(self.idx)
            iter = self.item.dereference()
            self.item += 1
            self.idx += 1
            return key, iter

    def __init__(self, val):
        self.val = val

    def to_string(self):
        return "std::vector of size %d, capacity %d" % (self.val['m_size'], self.val['m_capacity'])

    def display_hint(self):
        return 'array'

    def children(self):
        if self.val['m_size'] == 0:
            return []
        data, alloc = parseCompressedPair(self.val['m_data'])
        return self._iterator(data, data + self.val['m_size'], 0)

def _leftmost(node):
    ret = node
    while ret['left'] != 0:
        ret = ret['left'].dereference()
    return ret

def _next(node):
    if node['right']:
        return _leftmost(node['right'].dereference())
    else:
        if node['parent'] == 0:
            return None
        parent = node['parent'].dereference()
        if parent['left'] == node.address:
            return parent
        ret = node
        while True:
            ret = ret['parent'].dereference()
            if ret['parent'] == 0:
                return None
            if ret['parent'].dereference()['left'] == ret.address:
                break
        return ret['parent'].dereference()

class rbtreePrinter:
    def __init__(self, type, val):
        self.type: gdb.Type = type
        self.val: gdb.Value = val['tree']

    def to_string(self):
        return "%s of size %d" % (self.type, self.num_children())

    def display_hint(self):
        return 'array'

    def num_children(self):
        size, _ = parseCompressedPair(self.val['size_data'])
        return size

    def children(self):
        root, alloc = parseCompressedPair(self.val['root_data'])
        size, comp = parseCompressedPair(self.val['size_data'])

        # yield '[alloc]', alloc
        # yield '[comp]', comp
        # yield '[root]', root
        if root == 0:
            return

        nd = _leftmost(root.dereference())
        for i in range(size):
            yield str(i), nd['value']
            nd = _next(nd)
            if nd == None:
                break

class stringPrinter:
    def __init__(self, val: gdb.Value):
        self.val = val

    def to_string(self):
        data, alloc = parseCompressedPair(self.val['m_data'])
        data = data['in']

        if data['stackdata']['end'] == 0:
            return data['stackdata']['str'].string()
        return data['heapdata']['m_ptr'].string()

    def display_hint(self):
        return 'string'

class stringViewPrinter:
    def __init__(self, val: gdb.Value):
        self.val = val
        self.string = val['m_str']
        self.length = val['m_len']

    def to_string(self):
        return self.string.string(length=self.length)

    def display_hint(self):
        return 'string'

class listPrinter:
    def __init__(self, val):
        self.val: gdb.Value = val
        self.type: gdb.Type = val.type

        this_type = self.type.unqualified().strip_typedefs()
        if this_type.tag == None:
            this_type = this_type.target()

        self.value_node_type = gdb.lookup_type(this_type.tag + '::node').pointer()

    def to_string(self):
        size, alloc = parseCompressedPair(self.val['m_pair'])
        return 'std::list of size %d' % size

    def display_hint(self):
        return 'array'

    def num_children(self):
        size, alloc = parseCompressedPair(self.val['m_pair'])
        return size

    def children(self):
        head = self.val['m_head']

        node = head['next']
        idx = 0
        while node != head.address:
            nodeval = node.reinterpret_cast(self.value_node_type)
            yield str(idx), nodeval['value']
            idx += 1
            node = node['next']

class listIteratorPrinter:
    def __init__(self, val):
        self.val = val

        this_type: gdb.Type = val.type
        this_type = this_type.unqualified().strip_typedefs()

        if this_type.tag == None:
            this_type = this_type.target()

        type_tag: str = this_type.tag
        type_tag = type_tag[:type_tag.rfind('::')]

        self.value_node_type = gdb.lookup_type(type_tag + '::node').pointer()

    def children(self):
        yield 'addr', self.val['p']
        if self.val['p'] == 0:
            return

        nodeptr = self.val['p'].cast(self.value_node_type)

        yield 'value', nodeptr['value']

class rbtreeIteratorPrinter:
    def __init__(self, val):
        self.val = val

    def children(self):
        yield 'addr', self.val['p']
        if self.val['p'] == 0:
            return

        yield 'value', self.val['p']['value']

class vectorIteratorPrinter:
    def __init__(self, val):
        self.val = val

    def children(self):
        yield 'value', self.val['m_ptr'].dereference()

class pairPrinter:
    def __init__(self, val):
        self.val = val

    def children(self):
        yield 'first', self.val['first']
        yield 'second', self.val['second']

class tuplePrinter:
    def __init__(self, val):
        self.val = val

    def children(self):
        i = 0
        try:
            cur = self.val
            while True:
                yield '<%d>' % i, cur['val']
                i += 1
                cur = cur['next']
        except Exception:
            if i == 0:
                yield 'tuple of size 0', ''

class functionPrinter:
    def __init__(self, val: gdb.Value):
        self.val = val

    def to_string(self):
        return self.val.type.name

class referenceWrapperPrinter:
    def __init__(self, val):
        self.val = val

    def to_string(self):
        return "std::reference_wrapper to %x" % self.val['_ptr']

    def children(self):
        yield 'addr', self.val['_ptr'].cast(gdb.lookup_type('void').pointer())
        yield 'reference', self.val['_ptr']

class sharedPointerPrinter:
    def __init__(self, val: gdb.Value):
        self.val = val
        self.pointer = val['ptr']
        self.controlBlock = val['cb']

    def to_string(self):
        if self.pointer == 0:
            return 'nullptr of %s' % self.val.type.name

        refCount = self.controlBlock['ref_count']
        weakCount = self.controlBlock['weak_count']
        realPointer = self.controlBlock['ptr']
        return '%s to 0x%x, ref(%d), wref(%d), cb(0x%x), memp(0x%x)' % (
                self.val.type.name,
                self.pointer,
                refCount,
                weakCount,
                self.controlBlock,
                realPointer)

    def children(self):
        if self.pointer == 0:
            return []

        content = self.pointer.dereference()
        return delegateChildren(content)

class uniquePointerPrinter:
    def __init__(self, val: gdb.Value):
        self.val = val
        self.data = val['data']

    def to_string(self):
        pointer, deleter = parseCompressedPair(self.data)
        if pointer == 0:
            return 'nullptr of %s' % self.val.type.name

        return "%s to 0x%x" % (self.val.type.name, pointer)

    def children(self):
        pointer, deleter = parseCompressedPair(self.data)
        yield '[deleter]', deleter

        if pointer == 0:
            return

        for item in delegateChildren(pointer.dereference()):
            yield item

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

    if re.compile(r"^std::pair<.*, .*>$").match(typename):
        return pairPrinter(val)

    if re.compile(r"^std::tuple<.*>$").match(typename):
        return tuplePrinter(val)

    if re.compile(r"^std::function<.*>$").match(typename):
        return functionPrinter(val)

    if re.compile(r"^std::reference_wrapper<.*>$").match(typename):
        return referenceWrapperPrinter(val)

    if re.compile(r"^std::list<.*, .*>::_iterator<.*?>$").match(typename):
        return listIteratorPrinter(val)

    if re.compile(r"^std::vector<.*, .*>::_iterator<.*?>$").match(typename):
        return vectorIteratorPrinter(val)

    if re.compile(r"^std::list<.*, .*>$").match(typename):
        return listPrinter(val)

    if re.compile(r"^std::vector<.*, .*>$").match(typename):
        return vectorPrinter(val)

    if re.compile(r"^std::map<.*, .*, .*, .*>$").match(typename):
        return rbtreePrinter("std::map", val)

    if re.compile(r"^std::set<.*, .*, .*>$").match(typename):
        return rbtreePrinter("std::set", val)

    if re.compile(r"^std::impl::rbtree<.*, .*, .*>::_iterator<.*?>$").match(typename):
        return rbtreeIteratorPrinter(val)

    if re.compile(r"^std::basic_string<.*>$").match(typename):
        return stringPrinter(val)

    if re.compile(r"^types::string_view$").match(typename):
        return stringViewPrinter(val)

    if re.compile(r"^std::shared_ptr<.*>$").match(typename):
        return sharedPointerPrinter(val)

    if re.compile(r"^std::unique_ptr<.*>$").match(typename):
        return uniquePointerPrinter(val)

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
gdb.execute('skip -rfu "^gbos_rust_part::sync::lock::Lock<[a-zA-Z0-9_<>: ,]+, [a-zA-Z0-9_<>: ,]+>::new<[a-zA-Z0-9_<>: ,]+, [a-zA-Z0-9_<>: ,]+>"')
gdb.execute('skip -rfu "^gbos_rust_part::sync::locked::Locked<[a-zA-Z0-9_<>: ,]+, [a-zA-Z0-9_<>: ,]+>::new<[a-zA-Z0-9_<>: ,]+, [a-zA-Z0-9_<>: ,]+>"')
gdb.pretty_printers.append(build_pretty_printer)
