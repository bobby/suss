"""Fail-closed correspondence for the evaluated numeric closure example.

This is a representative mapping, not a general compiler. The suss.core/+ call
is specialized to numeric addition only for this pinned, oracle-checked profile;
mutable global replacement and arbitrary argument values are outside it.
"""
import re


def require(value, message):
    if not value:
        raise ValueError(message)


def strict_equal(actual, expected):
    """Closed JSON equality: bool/int and int/float are distinct types."""
    if type(actual) is not type(expected):
        return False
    if isinstance(expected, dict):
        return actual.keys() == expected.keys() and all(
            strict_equal(actual[key], value) for key, value in expected.items())
    if isinstance(expected, list):
        return len(actual) == len(expected) and all(
            strict_equal(a, b) for a, b in zip(actual, expected))
    return actual == expected


def children(node, roles):
    edges = node['children']
    require([edge['role'] for edge in edges] == roles, 'unexpected child order/operation')
    return [edge['node'] for edge in edges]


def result(node):
    require(node['op'] == 'do', 'expected single result body')
    return children(node, ['result'])[0]


def local(node, identity):
    require(node['op'] == 'local' and not node['children'], 'expected leaf local')
    require(node['resolved']['tag'] == 'local' and
            node['resolved']['binding']['bindingId'] == identity,
            'local identity does not match executable operand')


def number(node):
    require(node['op'] == 'literal' and not node['children'], 'expected numeric leaf')
    data = node['originalForm']['data']
    require(data['tag'] == 'f64' and re.fullmatch('[0-9a-f]{16}', data['bits']),
            'expected binary64 literal')
    return data['bits']


def verify(facts, graph):
    require(facts['schema'] == 'suss.source-analysis.draft.v1', 'wrong source schema')
    root = facts['selectedFacts']
    require(root['op'] == 'let', 'expected representative let')
    literal, closure, body = children(root, ['init', 'init', 'body'])
    declarations = root['declarations']
    require(len(declarations) == 2, 'unexpected let declarations')
    x, f = [declaration['bindingId'] for declaration in declarations]
    require(x != f and [edge.get('bindingId') for edge in root['children'][:2]] == [x, f],
            'initializer/declaration identity mismatch')
    require(all(strict_equal(declaration['kind'], {'tag': 'let'}) for declaration in declarations),
            'unsupported declaration kind')
    xb = number(literal)
    require(closure['op'] == 'closure' and not closure['children'], 'unsupported closure')
    require([capture['bindingId'] for capture in closure['captures']] == [x],
            'capture identity/order differs')
    callable = closure['callable']
    require(not callable['variadic'] and callable['recurs'] == {'present': False},
            'unsupported variadic/recursive closure')
    require(len(callable['parameters']) == len(callable['declarations']) == 1,
            'expected one closure argument')
    parameter = callable['declarations'][0]
    require(strict_equal(parameter['kind'], {'tag': 'argument', 'index': 0, 'rest': False}),
            'unsupported argument declaration')
    y = parameter['bindingId']
    require(y not in [x, f], 'argument aliases outer binding')
    add = result(callable['body'])
    require(add['op'] == 'invoke', 'expected addition invocation')
    callee, lhs, rhs = children(add, ['callee', 'argument', 'argument'])
    expected_global = {'name': '+', 'namespace': 'suss.core', 'phase': 'runtime'}
    require(callee['op'] == 'global' and not callee['children'] and
            callee['resolved']['global'] == expected_global and
            add['resolved']['global'] == expected_global,
            'unsupported global specialization')
    local(lhs, x)
    local(rhs, y)
    call = result(body)
    require(call['op'] == 'invoke', 'expected closure call')
    target, argument = children(call, ['callee', 'argument'])
    local(target, f)
    require(call['resolved']['binding']['bindingId'] == f, 'call binding mismatch')
    nb = number(argument)
    expected_producer = {
        'parameters': [], 'operations': [
            {'op': 'suss.const', 'bits': xb},
            {'op': 'suss.closure', 'arity': 1, 'captures': [0], 'body': {
                'parameters': [{'kind': 'f64'}, {'kind': 'f64'}],
                'operations': [{'op': 'suss.add', 'lhs': 0, 'rhs': 1}],
                'return_value': 2}}], 'return_value': 1}
    expected_caller = {
        'parameters': [{'kind': 'closure', 'arity': 1, 'captures': [{'kind': 'f64'}]}],
        'operations': [{'op': 'suss.const', 'bits': nb},
                       {'op': 'suss.call', 'callee': 0, 'arguments': [1]}],
        'return_value': 2}
    require(strict_equal(graph['producer'], expected_producer), 'producer operations differ from source mapping')
    require(strict_equal(graph['caller'], expected_caller), 'caller operations differ from source mapping')
    return {'schema': 'suss.representative-correspondence.v1',
            'captured_binding': x, 'closure_binding': f, 'argument_binding': y,
            'literal_bits': xb, 'call_argument_bits': nb,
            'specialization': 'pinned numeric suss.core/+; immutable fixture environment',
            'producer_source_span': closure['span'], 'caller_source_span': call['span'],
            'arithmetic_source_span': add['span']}
