#!/usr/bin/env python3
"""Strict host validation for durable #245 source/reference/native observations.

The seven original IDs/sources remain fixed. Three pinned public call-order
cases supplement them. This module never executes Suss or ClojureScript.
"""
from collections import Counter
import copy
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
CORPUS_PATH = ROOT / 'tests/oracle/reduction-dispatch-cases.json'
FIXTURE_PATH = ROOT / 'tests/oracle/fixtures/reduction-dispatch-fixture.sus'
EVIDENCE = ROOT / 'docs/compatibility/reduction-dispatch/evidence'
CLJS_ORIGINAL_PATH = EVIDENCE / 'clojurescript-original-seven.json'
CLJS_ORDER_PATH = EVIDENCE / 'clojurescript-source-order-supplement.json'
NATIVE_ORIGINAL_PATH = EVIDENCE / 'native-original-seven.json'
ORDER_PLAN_PATH = EVIDENCE / 'source-order-supplement-plan.json'
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
DECODER = 'portable_decode::Decoder raw ABI observations'
IDS = (
    'public-reduce-kv-arraymap-and-hamt-normal',
    'public-reduce-kv-arraymap-and-hamt-empty',
    'public-reduce-kv-arraymap-and-hamt-reduced-stop',
    'public-record-reduce-two-and-three-arguments',
    'public-queue-reduce-two-and-three-arguments',
    'public-set-reduce-two-and-three-arguments',
    'public-record-queue-set-reduce-reduced-stops-both-arities',
)
ORDER_IDS = (
    'public-reduce-two-arg-fn-before-coll',
    'public-reduce-three-arg-fn-init-coll',
    'public-reduce-kv-array-map-fn-init-coll',
)
ALL_IDS = IDS + ORDER_IDS
ORIGINAL_SOURCE_SHA256 = (
    'fed801a5bca88361d4ac19e9f00b1d883998f178d3a6688e340f81cb53927433',
    '838db844094e197069fc6b385b50d43681a09837ef34030f20aa7b55b8773c8d',
    '25860241723b2dd8e32d910a304c682af91e08227b8b9f2916648920ac8ffc64',
    '2aad512228992708080084d388d0334e80981e2a13bb87bca39ff2ff5af4ee25',
    'b848399adc74d7e41098bb092f6659793baea402ceb3949fea6a930e6a285c30',
    'cc34bf2134c99095cd01f6f7f8672d2ae63edb20f2a11198e4dca083e88562e0',
    'd29b99f818b487ba5258378761b3f11478c608c176440e07ed54b31e11ef8834',
)
ORDER_SOURCE_SHA256 = (
    'fb326d8c48a4f15deb8a57703ebfa07a2a8c000a55cce89f4094a772d18ca952',
    '0f4395604eed68ff48c730198a9e78eb5f10ebed41868c18f4fcbf0a7e66063c',
    '98e703f5d24b647498a3445b1ed762cfff4e7e3e1000ac903ce36908c1452ce2',
)
EXPECTED_CORPUS_SHA256 = 'a9999c1ebc1d006dfc2a204c18304127a2dda21bcab108c7409dd42bccabb8aa'
EXPECTED_FIXTURE_SHA256 = '7b62d5c79fe667cf4279d5ea2b491c1761895b8d3f1df33b9a246d4e265ed476'
EXPECTED_ORIGINAL_REFERENCE_SHA256 = '6e451c497f37a4c255e543b9354fae2c0d0189bc7db211f036279fc15ed0d8a4'
EXPECTED_ORDER_REFERENCE_SHA256 = 'f839f8328dba9742ddd35384308d0999e1c3a90e541e4163a1646bbf4be7d3f4'
EXPECTED_ORIGINAL_NATIVE_SHA256 = '204e53cb8dad0418e87a51fc78289835188ec3d3f50d2875b02f48edf24aab54'
EXPECTED_ORDER_PLAN_SHA256 = 'e540c7fd15c9d5f89210cf2d052f0765cb25dab06597fa8c857569b1531497e8'

sys.path.insert(0, str(ROOT / 'scripts'))
from oracle_transport import fields, unique, value  # noqa: E402


def check(ok, message):
    if not ok:
        raise AssertionError(message)


def read(path):
    return json.loads(Path(path).read_text(), object_pairs_hook=unique)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate_pinned_native_capture_bytes(data):
    check(hashlib.sha256(data).hexdigest()==EXPECTED_ORIGINAL_NATIVE_SHA256,
          'historical original-seven native capture SHA-256 mismatch')
    return json.loads(data,object_pairs_hook=unique)


def tagged_vector(items): return {'tag':'vector','items':items}
def number(bits): return {'tag':'f64','bits':bits}
def kw(name):
    return {'tag':'keyword','namespace':{'tag':'nil'},'name':{'tag':'string','units':[ord(c) for c in name]}}

def vec(node, length=None, where='vector'):
    check(type(node) is dict and set(node)=={'tag','items'} and node['tag']=='vector', f'{where}: expected exactly a tagged vector')
    items=node['items']; check(type(items) is list, f'{where}: expected vector items')
    if length is not None: check(len(items)==length, f'{where}: expected {length} items, got {len(items)}')
    return items

def f64(node,bits,where):
    check(type(node) is dict and set(node)=={'tag','bits'} and node['tag']=='f64', f'{where}: expected tagged binary64')
    check(node['bits']==bits, f'{where}: expected binary64 bits {bits}, got {node["bits"]}')

def boolean(node,expected,where):
    check(type(node) is dict and set(node)=={'tag','value'} and node['tag']=='bool',f'{where}: expected tagged Boolean')
    check(type(node['value']) is bool and node['value'] is expected,f'{where}: expected Boolean {expected}')

def keyword(node,name,where): check(node==kw(name),f'{where}: expected unqualified keyword :{name}')

def entry(key,bits): return tagged_vector([kw(key),number(bits)])

def encoded_multiset(items):
    return Counter(json.dumps(item,sort_keys=True,separators=(',',':')) for item in items)

def assert_unordered_exact(actual,expected,where):
    check(encoded_multiset(actual)==encoded_multiset(expected),f'{where}: unordered entries or multiplicities differ')

def assert_unique_subset(actual,domain,expected_count,where):
    counts=encoded_multiset(actual); allowed=set(encoded_multiset(domain))
    check(len(actual)==expected_count,f'{where}: expected {expected_count} entries')
    check(all(n==1 for n in counts.values()),f'{where}: duplicate input entry')
    check(set(counts).issubset(allowed),f'{where}: input outside source collection')

def expected_map_entries(hamt):
    if hamt:
        values = [('a', '3ff0000000000000'), ('b', '4000000000000000'),
                  ('c', '4008000000000000'), ('d', '4010000000000000'),
                  ('e', '4014000000000000'), ('f', '4018000000000000'),
                  ('g', '401c000000000000'), ('h', '4020000000000000'),
                  ('i', '4022000000000000'), ('j', '4024000000000000')]
    else:
        values = [('a', '4000000000000000'), ('b', '4008000000000000'),
                  ('c', '4014000000000000'), ('d', '401c000000000000')]
    return [entry(key, bits) for key, bits in values]


def stopped(node, where):
    keyword(node, 'stopped', where)



def validate_case(case_id, candidate, ref):
    if case_id == IDS[0]:
        actual_runs = vec(candidate, 2, case_id)
        reference_runs = vec(ref[case_id], 2, f'{case_id} pinned')
        for index, (actual, expected) in enumerate(zip(actual_runs, reference_runs)):
            where = f'{case_id} run {index}'
            actual = vec(actual, 4, where)
            expected = vec(expected, 4, f'{where} pinned')
            bits = ('4031000000000000', '404b800000000000')[index]
            f64(expected[0], bits, f'{where} pinned sum')
            f64(actual[0], bits, f'{where} sum')
            entries = vec(expected[1], where=f'{where} pinned callback entries')
            expected_entries = expected_map_entries(index == 1)
            check(len(entries) == len(expected_entries), f'{where}: pinned entry count differs from source')
            assert_unordered_exact(entries, expected_entries, f'{where} pinned source entries')
            actual_entries = vec(actual[1], where=f'{where} callback entries')
            assert_unordered_exact(actual_entries, expected_entries, f'{where} callback entries')
            boolean(expected[2], index == 0, f'{where} pinned PersistentArrayMap?')
            boolean(expected[3], index == 1, f'{where} pinned PersistentHashMap?')
            boolean(actual[2], index == 0, f'{where} PersistentArrayMap?')
            boolean(actual[3], index == 1, f'{where} PersistentHashMap?')
        return

    if case_id == IDS[1]:
        actual_runs = vec(candidate, 2, case_id)
        expected_runs = vec(ref[case_id], 2, f'{case_id} pinned')
        for index, (actual, expected) in enumerate(zip(actual_runs, expected_runs)):
            where = f'{case_id} run {index}'
            actual = vec(actual, 4, where)
            expected = vec(expected, 4, f'{where} pinned')
            f64(actual[0], '4031000000000000', f'{where} initial result')
            f64(actual[1], '0000000000000000', f'{where} callback count')
            f64(expected[0], '4031000000000000', f'{where} pinned result')
            f64(expected[1], '0000000000000000', f'{where} pinned callback count')
            boolean(actual[2], index == 0, f'{where} PersistentArrayMap?')
            boolean(actual[3], index == 1, f'{where} PersistentHashMap?')
            check(actual == expected, f'{where}: empty reduction differs from pinned observation')
        return

    if case_id == IDS[2]:
        actual_runs = vec(candidate, 2, case_id)
        expected_runs = vec(ref[case_id], 2, f'{case_id} pinned')
        normal_runs = vec(ref[IDS[0]], 2, 'pinned full map inputs')
        for index, (actual, expected, full) in enumerate(zip(actual_runs, expected_runs, normal_runs)):
            where = f'{case_id} run {index}'
            actual = vec(actual, 4, where)
            expected = vec(expected, 4, f'{where} pinned')
            stopped(actual[0], f'{where} result')
            stopped(expected[0], f'{where} pinned result')
            pair = vec(vec(actual[1], 1, f'{where} callback trace')[0], 2, f'{where} callback pair')
            domain = vec(vec(full, 4, f'{where} full input')[1], where=f'{where} full map entries')
            assert_unique_subset([{'tag': 'vector', 'items': pair}], domain, 1, f'{where} callback entry')
            boolean(actual[2], index == 0, f'{where} PersistentArrayMap?')
            boolean(actual[3], index == 1, f'{where} PersistentHashMap?')
            check(actual[2:] == expected[2:], f'{where}: public representation predicates differ')
        return

    if case_id == IDS[3]:
        actual = vec(candidate, 3, case_id)
        expected = vec(ref[case_id], 3, f'{case_id} pinned')
        f64(actual[0], '4044800000000000', f'{case_id} reduce without init')
        f64(actual[1], '4044800000000000', f'{case_id} reduce with init')
        boolean(actual[2], True, f'{case_id} public record predicate')
        check(actual == expected, f'{case_id}: result differs from pinned observation')
        return

    if case_id == IDS[4]:
        actual = vec(candidate, 5, case_id)
        expected = vec(ref[case_id], 5, f'{case_id} pinned')
        f64(actual[0], '4024000000000000', f'{case_id} no-init sum')
        check(actual[1] == {'tag': 'vector', 'items': [number(bits) for bits in
              ('4000000000000000', '4008000000000000', '4010000000000000')]},
              f'{case_id}: no-init trace is not exact FIFO [2,3,4]')
        f64(actual[2], '4024000000000000', f'{case_id} init sum')
        check(actual[3] == {'tag': 'vector', 'items': [number(bits) for bits in
              ('3ff0000000000000', '4000000000000000', '4008000000000000', '4010000000000000')]},
              f'{case_id}: init trace is not exact FIFO [1,2,3,4]')
        boolean(actual[4], True, f'{case_id} public queue predicate')
        check(actual == expected, f'{case_id}: queue reduction differs from pinned observation')
        return

    if case_id == IDS[5]:
        actual = vec(candidate, 7, case_id)
        expected = vec(ref[case_id], 7, f'{case_id} pinned')
        for index, where in ((0, 'no-init sum'), (3, 'init sum')):
            f64(actual[index], '4024000000000000', f'{case_id} {where}')
        f64(actual[1], '4008000000000000', f'{case_id} no-init count')
        f64(actual[4], '4010000000000000', f'{case_id} init count')
        domain = [number(bits) for bits in ('3ff0000000000000', '4000000000000000',
                                             '4008000000000000', '4010000000000000')]
        no_init = vec(actual[2], 3, f'{case_id} no-init inputs')
        with_init = vec(actual[5], 4, f'{case_id} init inputs')
        assert_unique_subset(no_init, domain, 3, f'{case_id} no-init inputs')
        assert_unordered_exact(with_init, domain, f'{case_id} init inputs')
        boolean(actual[6], True, f'{case_id} public set predicate')
        for index in (0, 1, 3, 4, 6):
            check(actual[index] == expected[index],
                  f'{case_id}: stable sum/count/predicate field {index} differs from pinned')
        # Set iteration order is intentionally excluded, but the captured CLJS
        # no-init subset must also be three distinct members of the source set.
        ref_no_init = vec(expected[2], 3, f'{case_id} pinned no-init inputs')
        ref_init = vec(expected[5], 4, f'{case_id} pinned init inputs')
        assert_unique_subset(ref_no_init, domain, 3, f'{case_id} pinned no-init inputs')
        assert_unordered_exact(ref_init, domain, f'{case_id} pinned init inputs')
        return

    if case_id == IDS[6]:
        actual = vec(candidate, 6, case_id)
        expected = vec(ref[case_id], 6, f'{case_id} pinned')
        record_domain = [entry('a', '4026000000000000'), entry('b', '402a000000000000'),
                         entry('c', '4031000000000000')]
        set_domain = [number(bits) for bits in ('3ff0000000000000', '4000000000000000',
                                                 '4008000000000000', '4010000000000000')]
        for index, (item, ref_item) in enumerate(zip(actual, expected)):
            where = f'{case_id} callback {index}'
            item = vec(item, 2, where)
            ref_item = vec(ref_item, 2, f'{where} pinned')
            stopped(item[0], f'{where} reduced result')
            stopped(ref_item[0], f'{where} pinned reduced result')
            trace = vec(item[1], 1, f'{where} trace (no later callback)')
            ref_trace = vec(ref_item[1], 1, f'{where} pinned trace')
            if index < 2:
                pair = vec(trace[0], 2, f'{where} record accumulator/entry')
                if index == 0:
                    assert_unique_subset([pair[0], pair[1]], record_domain, 2, where)
                    check(pair[0] != pair[1], f'{where}: no-init record accumulator repeated')
                else:
                    keyword(pair[0], 'seed', f'{where} explicit initial accumulator')
                    assert_unique_subset([pair[1]], record_domain, 1, where)
            elif index < 4:
                expected_first = '4000000000000000' if index == 2 else '3ff0000000000000'
                check(trace[0] == number(expected_first), f'{where}: queue callback is not FIFO-first')
                check(trace == ref_trace, f'{where}: queue trace differs from pinned raw capture')
            else:
                pair = vec(trace[0], 2, f'{where} set accumulator/input')
                if index == 4:
                    assert_unique_subset(pair, set_domain, 2, where)
                    check(pair[0] != pair[1], f'{where}: no-init set accumulator repeated')
                else:
                    keyword(pair[0], 'seed', f'{where} explicit initial accumulator')
                    assert_unique_subset([pair[1]], set_domain, 1, where)
        return

    raise AssertionError(f'unknown original case {case_id}')



def order_expected():
    call=lambda *xs: tagged_vector([kw('call'),*xs])
    return {
      ORDER_IDS[0]:tagged_vector([number('4008000000000000'),tagged_vector([kw('fn'),kw('coll'),call(number('3ff0000000000000'),number('4000000000000000'))])]),
      ORDER_IDS[1]:tagged_vector([number('402a000000000000'),tagged_vector([kw('fn'),kw('init'),kw('coll'),call(number('4024000000000000'),number('3ff0000000000000')),call(number('4026000000000000'),number('4000000000000000'))])]),
      ORDER_IDS[2]:tagged_vector([number('402a000000000000'),tagged_vector([kw('fn'),kw('init'),kw('coll'),call(number('4024000000000000'),kw('only'),number('4008000000000000'))])]),
    }


def validate_corpus(corpus):
    fields(corpus,'schema purpose fixture portable_reference source_guidance cases source_order_supplement independent_decoder_policy')
    check(type(corpus['schema']) is int and corpus['schema']==1,'wrong corpus schema')
    check(corpus['fixture']=='tests/oracle/fixtures/reduction-dispatch-fixture.sus','fixture path changed')
    refmeta=corpus['portable_reference']
    fields(refmeta,'language upstream_commit status original_seven_capture source_order_capture expected_values_embedded')
    check(refmeta['language']=='ClojureScript' and refmeta['upstream_commit']==PIN,'wrong reference pin')
    check(refmeta['expected_values_embedded'] is False,'raw captures must remain the reference values')
    check(refmeta['original_seven_capture']=={
        'artifact':'docs/compatibility/reduction-dispatch/evidence/clojurescript-original-seven.json',
        'sha256':EXPECTED_ORIGINAL_REFERENCE_SHA256},'original reference capture metadata mismatch')
    check(refmeta['source_order_capture']=={
        'artifact':'docs/compatibility/reduction-dispatch/evidence/clojurescript-source-order-supplement.json',
        'sha256':EXPECTED_ORDER_REFERENCE_SHA256},'source-order reference capture metadata mismatch')
    guidance=corpus['source_guidance']
    check(type(guidance) is list and len(guidance)>=4,'upstream source/license guidance missing')
    check(any(g.get('license')=='MPL-2.0' and g.get('license_file')=='vendor/clojure-test-suite/upstream/LICENSE' for g in guidance),'upstream MPL attribution missing')
    rows=corpus['cases']
    check(type(rows) is list and tuple(r.get('id') for r in rows)==IDS,'original seven IDs/order changed')
    by_id={}
    for row,source_sha in zip(rows,ORIGINAL_SOURCE_SHA256):
        fields(row,'id source host_validation expected_capture source_sha256')
        check(type(row['source']) is str and hashlib.sha256(row['source'].encode()).hexdigest()==source_sha,f'{row["id"]}: original source changed')
        check(row['source_sha256']==source_sha,f'{row["id"]}: recorded source hash mismatch')
        check(row['expected_capture']=={'artifact':'evidence/clojurescript-original-seven.json','case_id':row['id']},f'{row["id"]}: expected capture pointer changed')
        check(type(row['host_validation']) is str and row['host_validation'],f'{row["id"]}: host validation guidance missing')
        by_id[row['id']]=row
    supplemental=corpus['source_order_supplement']
    fields(supplemental,'status plan_artifact plan_sha256 cases')
    check(supplemental['plan_artifact']=='docs/compatibility/reduction-dispatch/evidence/source-order-supplement-plan.json','source-order plan pointer changed')
    check(supplemental['plan_sha256']==EXPECTED_ORDER_PLAN_SHA256,'source-order plan pin mismatch')
    order_rows=supplemental['cases']
    check(type(order_rows) is list and tuple(r.get('id') for r in order_rows)==ORDER_IDS,'source-order IDs/order changed')
    for row,source_sha in zip(order_rows,ORDER_SOURCE_SHA256):
        fields(row,'id source source_sha256 expected_capture host_validation')
        check(hashlib.sha256(row['source'].encode()).hexdigest()==source_sha and row['source_sha256']==source_sha,f'{row["id"]}: source changed')
        check(row['expected_capture']=={'artifact':'evidence/clojurescript-source-order-supplement.json','case_id':row['id']},f'{row["id"]}: source-order capture pointer changed')
        check(type(row['host_validation']) is str and row['host_validation'],f'{row["id"]}: source-order validation guidance missing')
    return by_id,{r['id']:r for r in order_rows}


def load_inputs(check_bundle=True, corpus_path=CORPUS_PATH):
    if check_bundle:
        check(digest(corpus_path)==EXPECTED_CORPUS_SHA256,'durable corpus SHA-256 mismatch')
        check(digest(FIXTURE_PATH)==EXPECTED_FIXTURE_SHA256,'original fixture bytes changed')
        check(digest(CLJS_ORIGINAL_PATH)==EXPECTED_ORIGINAL_REFERENCE_SHA256,'original pinned CLJS raw capture hash mismatch')
        check(digest(CLJS_ORDER_PATH)==EXPECTED_ORDER_REFERENCE_SHA256,'source-order pinned CLJS raw capture hash mismatch')
        check(digest(ORDER_PLAN_PATH)==EXPECTED_ORDER_PLAN_SHA256,'source-order plan hash mismatch')
        check(digest(NATIVE_ORIGINAL_PATH)==EXPECTED_ORIGINAL_NATIVE_SHA256,
              'historical original-seven native capture SHA-256 mismatch')
    corpus=read(corpus_path); by_id,order_by_id=validate_corpus(corpus)
    original=read(CLJS_ORIGINAL_PATH); order=read(CLJS_ORDER_PATH)
    fields(original,'schema upstream cases'); check(original['schema']==2 and original['upstream']==PIN,'original CLJS capture pin/schema mismatch')
    fields(order,'schema upstream cases'); check(order['schema']==1 and order['upstream']==PIN,'source-order CLJS capture pin/schema mismatch')
    ref={}
    for document,ids,name in ((original,IDS,'original-seven'),(order,ORDER_IDS,'source-order')):
        rows=document['cases']; check(type(rows) is list and tuple(r.get('id') for r in rows)==ids,f'{name}: raw capture IDs/order mismatch')
        for row in rows:
            fields(row,'id status value effects')
            check(row['status']=='value',f'{row["id"]}: reference status is not value')
            check(row['effects']==[],f'{row["id"]}: unexpected transport-level effects')
            value(row['value']); ref[row['id']]=row['value']
    for case_id in IDS: validate_case(case_id,ref[case_id],ref)
    exact=order_expected()
    for case_id in ORDER_IDS:
        check(ref[case_id]==exact[case_id],f'{case_id}: raw pinned result/effect order differs from authored expectation')
    return corpus,by_id,order_by_id,original,order,ref


def validate_case_any(case_id,candidate,ref):
    if case_id in IDS: return validate_case(case_id,candidate,ref)
    check(case_id in ORDER_IDS,f'unknown case {case_id}')
    check(candidate==ref[case_id],f'{case_id}: native raw value/effect trace differs from pinned CLJS')
    check(candidate==order_expected()[case_id],f'{case_id}: candidate differs from explicit source-order contract')


def validate_observations(document,ref,ids=ALL_IDS,label='native'):
    fields(document,'decoder phases'); check(document['decoder']==DECODER,f'{label}: wrong decoder')
    phases=document['phases']; check(type(phases) is list and len(phases)==2,f'{label}: need Runtime and Macro phases')
    check([p.get('phase') for p in phases]==['runtime','macro'],f'{label}: phase order mismatch')
    results=[]
    for phase in phases:
        fields(phase,'phase cases'); rows=phase['cases']
        check(type(rows) is list and tuple(r.get('id') for r in rows)==tuple(ids),f'{label}/{phase["phase"]}: IDs/order mismatch')
        for row in rows:
            fields(row,'id value'); value(row['value']); validate_case_any(row['id'],row['value'],ref)
            results.append({'phase':phase['phase'],'id':row['id'],'matched':True})
    return results


def reference_envelope(ref):
    return {'decoder':DECODER,'phases':[{'phase':p,'cases':[{'id':i,'value':copy.deepcopy(ref[i])} for i in ALL_IDS]} for p in ('runtime','macro')]}


def negative_controls(ref):
    good=reference_envelope(ref); outcomes=[]
    def reject(name,mutate,ids=ALL_IDS):
        bad=copy.deepcopy(good); mutate(bad)
        try: validate_observations(bad,ref,ids,f'negative:{name}')
        except AssertionError as exc: outcomes.append({'name':name,'rejected':True,'reason':str(exc)})
        else: raise AssertionError(f'negative control accepted: {name}')
    def phase(doc,name='runtime'): return next(p for p in doc['phases'] if p['phase']==name)
    def case(doc,i,name='runtime'): return next(c for c in phase(doc,name)['cases'] if c['id']==i)['value']
    reject('missing-case-id',lambda d:phase(d)['cases'].pop())
    def duplicate_map(d):
        rows=case(d,IDS[0])['items'][0]['items'][1]['items']; rows[1]=copy.deepcopy(rows[0])
    reject('duplicate-map-entry-count-preserved',duplicate_map)
    def wrong_map_count(d): case(d,IDS[0])['items'][0]['items'][1]['items'].pop()
    reject('map-entry-count',wrong_map_count)
    def wrong_record_seed(d): case(d,IDS[6])['items'][1]['items'][1]['items'][0]['items'][0]=kw('wrong-seed')
    reject('wrong-record-seed',wrong_record_seed)
    def later_callback(d):
        trace=case(d,IDS[2])['items'][0]['items'][1]['items']; trace.append(copy.deepcopy(trace[0]))
    reject('callback-after-reduced',later_callback)
    def queue_fifo(d):
        trace=case(d,IDS[4])['items'][1]['items']; trace[1],trace[2]=trace[2],trace[1]
    reject('queue-fifo-order',queue_fifo)
    def float_bits(d): case(d,IDS[3])['items'][0]['bits']='4044800000000001'
    reject('binary64-bit-change',float_bits)
    def source_pin():
        corpus=copy.deepcopy(read(CORPUS_PATH)); corpus['cases'][0]['source']+=' '
        try: validate_corpus(corpus)
        except AssertionError as exc: outcomes.append({'name':'source-pin','rejected':True,'reason':str(exc)})
        else: raise AssertionError('source pin accepted mutated query')
    source_pin()
    changed_native=bytearray(NATIVE_ORIGINAL_PATH.read_bytes()); changed_native[-1] ^= 1
    try: validate_pinned_native_capture_bytes(bytes(changed_native))
    except AssertionError as exc: outcomes.append({'name':'changed-historical-native-capture','rejected':True,'reason':str(exc)})
    else: raise AssertionError('changed historical native capture accepted')
    return outcomes


def main(argv=None):
    argv=sys.argv[1:] if argv is None else argv
    corpus,by_id,order_by_id,original,order,ref=load_inputs()
    if argv==['--check-references']:
        native=validate_pinned_native_capture_bytes(NATIVE_ORIGINAL_PATH.read_bytes())
        rows=validate_observations(native,ref,IDS,'captured-original-seven')
        controls=negative_controls(ref)
        print(json.dumps({'status':'valid','original_native_matches':len(rows),'source_order_reference_cases':len(ORDER_IDS),'negative_controls_rejected':len(controls)},indent=2))
        return 0
    if argv==['--compare-stdin']:
        document=json.load(sys.stdin,object_pairs_hook=unique)
        rows=validate_observations(document,ref,ALL_IDS,'integration-test')
        print(json.dumps({'status':'valid','native_matches':len(rows),'cases_per_phase':len(ALL_IDS),'phases':['runtime','macro']}))
        return 0
    print('usage: reduction_dispatch_oracle.py --check-references|--compare-stdin',file=sys.stderr)
    return 2

if __name__=='__main__':
    try: raise SystemExit(main())
    except (AssertionError,ValueError,KeyError) as error:
        print(f'reduction-dispatch validation failed: {error}',file=sys.stderr)
        raise SystemExit(1)
