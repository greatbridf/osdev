import gdb.printing
import re

def create_iter(item, end, idx):
    return vectorPrinter._iterator(item, end, idx)

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
            key = '[%d]' % self.idx
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
            return [ ('<vector of size 0>', '') ]
        return self._iterator(self.val['m_data'], self.val['m_data'] + self.val['m_size'], 0)

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
        self.type = type
        self.val = val['tree']

    def to_string(self):
        return "%s of size %d" % (self.type, self.val['_size'])

    def display_hint(self):
        return 'array'

    def children(self):
        yield 'root', self.val['root']
        if self.val['root'] == 0:
            return

        yield 'size', self.val['_size']

        nd = _leftmost(self.val['root'].dereference())
        i = 0
        while True:
            yield "[%d]" % i, nd['value']
            nd = _next(nd)
            i += 1
            if nd == None:
                break

class stringPrinter:
    def __init__(self, val):
        self.val = val
    
    def to_string(self):
        return self.val['m_data']
    
    def children(self):
        yield 'str', self.val['m_data']

        if self.val['m_data'] == 0:
            return

        yield 'length', self.val['m_size'] - 1

        ptr = self.val['m_data']
        i = 0

        while ptr.dereference() != 0:
            yield '[%d]' % i, ptr.dereference()
            ptr += 1
            i += 1

        yield '[%d]' % i, 0

class listPrinter:
    def __init__(self, val):
        self.val = val
    
    def to_string(self):
        return "std::list of size %d" % self.val['m_size']

    def display_hint(self):
        return 'array'

    def children(self):
        head = self.val['m_head']

        yield 'head', head.address

        node = head['next']
        idx = 0
        while node != head.address:
            nodeval = node.reinterpret_cast(
                gdb.lookup_type(self.val.type.unqualified().strip_typedefs().tag + '::node').pointer()
                )
            yield '[%d]' % idx, nodeval['value']
            idx += 1
            node = node['next']

class listIteratorPrinter:
    def __init__(self, val):
        self.val = val
    
    def children(self):
        yield 'addr', self.val['p']
        if self.val['p'] == 0:
            return
        
        nodeptr = self.val['p']
        iter_type = self.val.type.unqualified().strip_typedefs().tag
        iter_type = iter_type[:iter_type.rfind('::')]
        nodeptr = nodeptr.cast(gdb.lookup_type(iter_type + '::node').pointer())
        
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
    def __init__(self, val):
        self.val = val
    
    def to_string(self):
        return self.val.type.tag
    
    def children(self):
        print(self.val['_data'].type)
        yield 'function data', self.val['_data']

class referenceWrapperPrinter:
    def __init__(self, val):
        self.val = val
    
    def to_string(self):
        return "std::reference_wrapper to %x" % self.val['_ptr']
    
    def children(self):
        yield 'addr', self.val['_ptr'].cast(gdb.lookup_type('void').pointer())
        yield 'reference', self.val['_ptr']

def build_pretty_printer(val):
    type = val.type

    if type.code == gdb.TYPE_CODE_REF:
        type = type.target()
    if type.code == gdb.TYPE_CODE_PTR:
        type = type.target()
    
    type = type.unqualified().strip_typedefs()
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

    # if re.compile(r"^std::list<.*, .*>::node$").match(typename):
    #     return None
    
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

    if re.compile(r"^types::string<.*>$").match(typename):
        return stringPrinter(val)
    
    return None

gdb.pretty_printers.append(build_pretty_printer)
