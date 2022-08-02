import gdb.printing

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

class stringPrinter:
    def __init__(self, val):
        self.val = val
    
    def to_string(self):
        return self.val['m_arr']

class listPrinter:
    def __init__(self, val):
        self.val = val
    
    def to_string(self):
        return "list of size %d" % (self.val['head'].reinterpret_cast(gdb.lookup_type("size_t").pointer()) + 2).dereference()

    def display_hint(self):
        return 'array'

    def children(self):
        node = self.val['head']['next']
        end = self.val['tail']
        idx = 0
        while node != end:
            yield '[%d]' % idx, node['value']
            idx += 1
            node = node['next']

def build_pretty_printer():
    pp = gdb.printing.RegexpCollectionPrettyPrinter("gbos")
    pp.add_printer("vector", "^types::vector<.*?>$", vectorPrinter)
    pp.add_printer("string", "^types::string<.*?>$", stringPrinter)
    pp.add_printer("list", "^types::list<.*?>$", listPrinter)
    return pp

gdb.printing.register_pretty_printer(
        gdb.current_objfile(),
        build_pretty_printer())
