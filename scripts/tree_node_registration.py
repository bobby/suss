"""Exact whole seven-target IComparable adaptation, not general macro expansion."""
from cljs_inventory import Scanner
TARGETS = ['Symbol', 'Keyword', 'Subvec', 'PersistentVector', 'MapEntry', 'BlackNode', 'RedNode']
ERROR = '(throw (js/Error. (str_ "Cannot compare " x " to " y)))'
PORTABLE_ERROR = '''(let [compare-x-text__ (str_ x) compare-y-text__ (str_ y)]
      (throw (suss.bootstrap/error
        (.join (cljs.core/array "Cannot compare " compare-x-text__ " to " compare-y-text__) ""))))'''

def expand(text):
    forms = Scanner(text).all()
    if len(forms) != 1: raise ValueError('one whole IComparable parent required')
    children = forms[0].children
    if [c.text for c in children[:2]] != ['extend-protocol', 'IComparable'] or len(children) != 16:
        raise ValueError('complete seven-target IComparable parent required')
    result = []
    targets = [c.text for c in children[2::2]]
    if targets != TARGETS: raise ValueError('complete ordered seven targets required')
    for target, method in zip(targets, children[3::2]):
        original = text[method.start:method.end]
        if original.count(ERROR) != 1: raise ValueError('original ordered rejection expression required')
        result.append('(extend-type ' + target + ' IComparable\n  ' + original.replace(ERROR, PORTABLE_ERROR) + ')')
    return '(do\n' + '\n\n'.join(result) + ')'
