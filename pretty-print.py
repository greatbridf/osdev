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
        return "vector of size %d, capacity %d" % (self.val['m_size'], self.val['m_capacity'])

    def display_hint(self):
        return 'array'

    def children(self):
        return self._iterator(self.val['m_arr'], self.val['m_arr'] + self.val['m_size'], 0)

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

class mapPrinter:
    def __init__(self, val):
        self.val = val

    def to_string(self):
        return "types::map"

    def display_hint(self):
        return 'array'

    def children(self):
        yield '[root]', self.val['root']
        if self.val['root'] == 0:
            return

        nd = _leftmost(self.val['root'].dereference())
        i = 0
        while True:
            yield "[%d]" % i, nd['v']
            nd = _next(nd)
            i += 1
            if nd == None:
                break

class stringPrinter:
    def __init__(self, val):
        self.val = val
    
    def to_string(self):
        return self.val['m_arr']
    
    def children(self):
        yield 'str', self.val['m_arr']

        if self.val['m_arr'] == 0:
            return

        yield 'length', self.val['m_size'] - 1

        ptr = self.val['m_arr']
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
        return "list of size %d" % (self.val['head'].reinterpret_cast(gdb.lookup_type("size_t").pointer()) + 2).dereference()

    def display_hint(self):
        return 'array'

    def children(self):
        head = self.val['head']
        end = self.val['tail']

        yield '[head]', head
        yield '[tail]', end
        
        if head == 0 or end == 0:
            return

        node = head['next']
        idx = 0
        while node != end:
            yield '[%d]' % idx, node['value']
            idx += 1
            node = node['next']

class listIteratorPrinter:
    def __init__(self, val):
        self.val = val
    
    def children(self):
        yield '[addr]', self.val['n']
        if self.val['n'] == 0:
            return

        for field in self.val['n']['value'].type.fields():
            yield field.name, self.val['n']['value'][field.name]

class mapIteratorPrinter:
    def __init__(self, val):
        self.val = val
    
    def children(self):
        yield '[addr]', self.val['p']
        if self.val['p'] == 0:
            return
        
        yield '[first]', self.val['p']['v']['first']
        yield '[second]', self.val['p']['v']['second']

class vectorIteratorPrinter:
    def __init__(self, val):
        self.val = val
    
    def children(self):
        yield 'value', self.val['p'].dereference()

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

    if re.compile(r"^types::list<.*?>::node<.*?>$").match(typename):
        return None

    if re.compile(r"^types::map<.*?,.*?,.*?>::iterator<.*?>$").match(typename):
        return mapIteratorPrinter(val)
    
    if re.compile(r"^types::list<.*?>::iterator<.*?>$").match(typename):
        return listIteratorPrinter(val)

    if re.compile(r"^types::vector<.*?>::iterator<.*?>$").match(typename):
        return vectorIteratorPrinter(val)

    if re.compile(r"^types::list<.*?>$").match(typename):
        return listPrinter(val)

    if re.compile(r"^types::vector<.*?>$").match(typename):
        return vectorPrinter(val)

    if re.compile(r"^types::string<.*?>$").match(typename):
        return stringPrinter(val)

    if re.compile(r"^types::map<.*?>$").match(typename):
        return mapPrinter(val)
    
    return None

gdb.pretty_printers.append(build_pretty_printer)
