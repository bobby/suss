// Original bounded transport validation, repository MIT/Apache-2.0 terms.
// Validates selected source facts, never derives source facts from runtime SSA.
#pragma once
#include "llvm/Support/JSON.h"
#include "llvm/ADT/StringSet.h"
#include <map>
#include <set>
namespace suss::analysis {
using J = llvm::json::Value;
using O = llvm::json::Object;
using A = llvm::json::Array;
// Preflight before LLVM's JSON object parser: reject duplicate decoded keys and
// bound nesting before allocating the parsed tree. Full syntax is checked later.
class UniqueKeys {
  llvm::StringRef text;
  size_t pos = 0;
  void space() { while (pos < text.size() && llvm::is_contained(" \n\r\t", text[pos])) ++pos; }
  bool take(char c) { space(); if (pos == text.size() || text[pos] != c) return false; ++pos; return true; }
  bool string(std::string &value) {
    space(); size_t start = pos;
    if (pos == text.size() || text[pos++] != '"') return false;
    while (pos < text.size()) {
      char c = text[pos++];
      if (c == '\\') { if (pos == text.size()) return false; ++pos; }
      else if (c == '"') {
        auto parsed = llvm::json::parse(text.slice(start, pos));
        if (!parsed) { llvm::consumeError(parsed.takeError()); return false; }
        auto s = parsed->getAsString(); if (!s) return false;
        value = s->str(); return true;
      }
    }
    return false;
  }
  bool value(unsigned depth) {
    if (depth > 128) return false;
    space(); if (pos == text.size()) return false;
    if (text[pos] == '{') {
      ++pos; llvm::StringSet<> keys; if (take('}')) return true;
      do {
        std::string key;
        if (!string(key) || !keys.insert(key).second || !take(':') || !value(depth + 1)) return false;
        if (take('}')) return true;
      } while (take(','));
      return false;
    }
    if (text[pos] == '[') {
      ++pos; if (take(']')) return true;
      do { if (!value(depth + 1)) return false; if (take(']')) return true; } while (take(','));
      return false;
    }
    if (text[pos] == '"') { std::string unused; return string(unused); }
    size_t start = pos;
    while (pos < text.size() && !llvm::is_contained(",]} \n\r\t", text[pos])) ++pos;
    return pos != start;
  }
public:
  explicit UniqueKeys(llvm::StringRef text) : text(text) {}
  bool check() { if (!value(0)) return false; space(); return pos == text.size(); }
};

class Validator {
  size_t remaining = 32768;
  std::map<std::string, J> bindings;
  std::map<uint64_t, std::string> hirIdentities;
public:
  std::string error;
  bool need(bool ok, llvm::StringRef message) { if (!ok && error.empty()) error = message.str(); return ok; }
  bool enter(unsigned d) { return need(d <= 64 && remaining-- > 0, "source-analysis depth/visit limit exceeded"); }
  const O *object(const J &v, std::initializer_list<llvm::StringRef> keys) {
    auto *o = v.getAsObject();
    if (!need(o && o->size() == keys.size(), "source-analysis missing/unknown object fields")) return nullptr;
    for (auto k : keys) if (!need(o->get(k), "source-analysis missing/unknown object fields")) return nullptr;
    return o;
  }
  bool text(const J &v) { return need(bool(v.getAsString()), "source-analysis requires string"); }
  bool flag(const J &v) { return need(bool(v.getAsBoolean()), "source-analysis requires boolean"); }
  bool integer(const J &v) { return need(bool(v.getAsUINT64()), "source-analysis requires nonnegative integer"); }
  bool span(const J &v) {
    auto *a = v.getAsArray();
    return need(a && a->size() == 2, "source-analysis requires half-open span") &&
      integer((*a)[0]) && integer((*a)[1]) && need(*(*a)[0].getAsUINT64() <= *(*a)[1].getAsUINT64(), "source-analysis reversed span");
  }
  bool enumeration(const J &v, std::initializer_list<llvm::StringRef> values) {
    auto s = v.getAsString(); return need(s && llvm::is_contained(values, *s), "source-analysis unsupported tag/context/phase");
  }
  bool context(const J &v) { return enumeration(v, {"statement", "expression", "return"}); }
  bool type(const J &v) {
    auto *o = v.getAsObject(); auto t = o ? o->getString("tag") : std::nullopt;
    if (!need(bool(t), "source-analysis physicalType missing tag")) return false;
    if (*t == "closure") return object(v, {"tag", "arity"}) && integer(*o->get("arity"));
    return object(v, {"tag"}) && enumeration(*o->get("tag"), {"nil", "bool", "number", "string", "value"});
  }
  bool forms(const J &v, unsigned d) {
    auto *a = v.getAsArray(); if (!need(a, "source-analysis requires form array")) return false;
    for (auto &f : *a) if (!form(f, d)) return false;
    return true;
  }
  bool form(const J &v, unsigned d) {
    if (!enter(d)) return false;
    auto *o = object(v, {"span", "metadata", "data"});
    if (!o || !span(*o->get("span")) || !forms(*o->get("metadata"), d+1)) return false;
    const auto &data = *o->get("data"); auto *b = data.getAsObject();
    auto tag = b ? b->getString("tag") : std::nullopt;
    if (!need(bool(tag), "source-analysis form requires data tag")) return false;
    if (*tag == "nil") return object(data, {"tag"});
    if (*tag == "bool") return object(data, {"tag", "value"}) && flag(*b->get("value"));
    if (*tag == "f64") {
      if (!object(data, {"tag", "bits"})) return false;
      auto bits = b->getString("bits");
      return need(bits && bits->size() == 16 && llvm::all_of(*bits, [](char c) { return (c>='0' && c<='9') || (c>='a' && c<='f'); }), "source-analysis f64 requires exact lowercase bits");
    }
    if (*tag == "utf16") {
      if (!object(data, {"tag", "units"})) return false;
      auto *units = b->getArray("units"); if (!need(units, "source-analysis UTF16 requires units array")) return false;
      for (auto &u : *units) if (!integer(u) || !need(*u.getAsUINT64() <= 65535, "source-analysis UTF16 unit out of range")) return false;
      return true;
    }
    if (*tag == "symbol" || *tag == "keyword")
      return object(data, {"tag", "namespace", "name"}) &&
        (b->get("namespace")->getAsNull() || text(*b->get("namespace"))) && text(*b->get("name"));
    if ((*tag=="list" || *tag=="vector" || *tag=="set" || *tag=="map" || *tag=="conditional"))
      return object(data, {"tag", "items"}) && forms(*b->get("items"), d+1);
    if (*tag == "discard") return object(data, {"tag", "target"}) && form(*b->get("target"), d+1);
    if (*tag == "prefix") return object(data, {"tag", "operator", "target"}) && form(*b->get("operator"), d+1) && form(*b->get("target"), d+1);
    return need(false, "source-analysis unsupported form tag");
  }
  bool optional(const J &v, unsigned d, bool nullable = false, bool boolean = false) {
    auto *o = v.getAsObject(); auto p = o ? o->getBoolean("present") : std::nullopt;
    if (!need(bool(p), "source-analysis optional requires present boolean")) return false;
    if (!*p) return object(v, {"present"});
    if (!object(v, {"present", "value"})) return false;
    auto &x = *o->get("value");
    return (nullable && x.getAsNull()) || (boolean ? flag(x) : form(x, d));
  }
  bool identity(const J &v) {
    auto s = v.getAsString();
    if (!need(s && s->starts_with("binding:"), "source-analysis bindingId is not declaration identity")) return false;
    auto n = s->drop_front(8);
    return need(!n.empty() && (n.size()==1 || n.front()!='0') && llvm::all_of(n, [](char c) { return c>='0' && c<='9'; }), "source-analysis malformed bindingId");
  }
  bool binding(const J &v, unsigned d) {
    if (!enter(d)) return false;
    auto *o = object(v, {"bindingId", "hirBindingId", "physicalType", "declaration", "kind", "context", "initializer", "shadow"});
    if (!o || !identity(*o->get("bindingId")) || !integer(*o->get("hirBindingId")) || !type(*o->get("physicalType")) ||
        !form(*o->get("declaration"), d+1) || !context(*o->get("context"))) return false;
    auto *declaration = o->getObject("declaration")->getObject("data");
    if (!need(declaration->getString("tag")=="symbol" && bool(declaration->get("namespace")->getAsNull()), "source-analysis local declaration requires unqualified symbol")) return false;
    auto id = o->getString("bindingId")->str(); auto hir = *o->get("hirBindingId")->getAsUINT64();
    // Source declaration identity survives physical binding remapping. Compare
    // every retained declaration fact except the current lowered HIR ID;
    // each concrete HIR ID must still name only this lexical declaration.
    J declarationFacts = v;
    declarationFacts.getAsObject()->erase("hirBindingId");
    auto prior = bindings.emplace(id, declarationFacts);
    if (!need(prior.second || prior.first->second == declarationFacts, "source-analysis conflicting declaration identity")) return false;
    auto hi = hirIdentities.emplace(hir, id);
    if (!need(hi.second || hi.first->second == id, "source-analysis conflicting hirBindingId")) return false;
    auto *kind = o->getObject("kind"); auto tag = kind ? kind->getString("tag") : std::nullopt;
    if (!need(bool(tag), "source-analysis binding requires kind")) return false;
    if (*tag == "let") {
      if (!object(*o->get("kind"), {"tag"}) || !need(!o->get("initializer")->getAsNull(), "source-analysis let requires initializer") || !node(*o->get("initializer"), d+1)) return false;
    } else if (*tag == "argument") {
      if (!object(*o->get("kind"), {"tag", "index", "rest"}) || !integer(*kind->get("index")) || !need(kind->getBoolean("rest") == false, "source-analysis rest argument unsupported") ||
          !need(bool(o->get("initializer")->getAsNull()), "source-analysis argument initializer unsupported")) return false;
    } else return need(false, "source-analysis unsupported declaration kind");
    return o->get("shadow")->getAsNull() || binding(*o->get("shadow"), d+1);
  }
  bool locals(const J &v, unsigned d) {
    auto *a = v.getAsArray(); if (!need(a, "source-analysis locals requires array")) return false;
    std::string last; bool first = true;
    for (auto &x : *a) {
      auto *o = object(x, {"name", "binding"}); if (!o || !text(*o->get("name"))) return false;
      auto name = o->getString("name")->str();
      if (!need(first || last < name, "source-analysis locals must be name-sorted and unique") || !binding(*o->get("binding"), d)) return false;
      auto *decl=o->getObject("binding")->getObject("declaration")->getObject("data");
      if (!need(decl->getString("tag")=="symbol" && decl->getString("name")==name && bool(decl->get("namespace")->getAsNull()), "source-analysis local name/declaration mismatch")) return false;
      last = name; first = false;
    }
    return true;
  }
  bool visible(const A &locals, llvm::StringRef id, uint64_t hir) {
    for (auto &x : locals) {
      auto *b = x.getAsObject()->getObject("binding");
      if (b->getString("bindingId") == id && b->get("hirBindingId")->getAsUINT64() == hir) return true;
    }
    return false;
  }
  using Scope = std::map<std::string, const J *>;
  Scope scopeIndex(const A &locals) {
    Scope result;
    for (auto &x : locals) {
      auto *local=x.getAsObject(); result[local->getString("name")->str()] = local->get("binding");
    }
    return result;
  }
  bool sameScope(const A &locals, const Scope &expected) {
    if (!need(locals.size()==expected.size(), "source-analysis child scope disagrees with declarations")) return false;
    for (auto &x : locals) {
      auto *local=x.getAsObject(); auto it=expected.find(local->getString("name")->str());
      if (!need(it!=expected.end() && *it->second==*local->get("binding"), "source-analysis child scope disagrees with declarations")) return false;
    }
    return true;
  }
  bool extendScope(const J &binding, Scope &scope) {
    auto *b=binding.getAsObject(); auto *decl=b->getObject("declaration")->getObject("data");
    auto name=decl->getString("name")->str(); auto old=scope.find(name);
    auto *shadow=b->get("shadow");
    if (!need(old==scope.end() ? bool(shadow->getAsNull()) : *shadow==*old->second, "source-analysis shadow disagrees with lexical scope")) return false;
    scope[name]=&binding; return true;
  }
  bool callable(const J &v, unsigned d) {
    auto *o = object(v, {"form", "declarations", "parameters", "variadic", "recurs", "entryLocals", "entryContext", "body"});
    if (!o || !form(*o->get("form"), d+1) || !need(o->getBoolean("variadic") == false, "source-analysis variadic callable unsupported") ||
        !optional(*o->get("recurs"), d+1, false, true) || !locals(*o->get("entryLocals"), d+1) || !context(*o->get("entryContext"))) return false;
    auto *ds = o->getArray("declarations"); auto *ps = o->getArray("parameters");
    if (!need(ds && ps && ds->size()==ps->size(), "source-analysis parameter declarations mismatch")) return false;
    for (size_t i=0;i<ds->size();++i) {
      if (!binding((*ds)[i], d+1)) return false;
      auto *b=(*ds)[i].getAsObject(); auto *p=object((*ps)[i], {"hirBindingId", "name", "span", "metadata"});
      if (!p || !integer(*p->get("hirBindingId")) || !text(*p->get("name")) || !span(*p->get("span")) || !forms(*p->get("metadata"), d+1)) return false;
      auto *k=b->getObject("kind"); auto *f=b->getObject("declaration"); auto *data=f->getObject("data");
      if (!need(k->getString("tag")=="argument" && k->get("index")->getAsUINT64()==i && *b->get("hirBindingId")==*p->get("hirBindingId") &&
          data->getString("tag")=="symbol" && data->getString("name")==p->getString("name") && *f->get("span")==*p->get("span") && *f->get("metadata")==*p->get("metadata"), "source-analysis parameter facts disagree")) return false;
    }
    if (!node(*o->get("body"), d+1)) return false;
    // The method entry snapshot precedes parameter allocation. Its analyzed
    // synthetic body must see precisely those locals plus the source parameters.
    auto expected=scopeIndex(*o->getArray("entryLocals"));
    for (auto &b : *ds) if (!extendScope(b, expected)) return false;
    auto *bodyLocals=o->getObject("body")->getArray("locals");
    if (!need(bodyLocals->size()==expected.size(), "source-analysis callable body scope disagrees with declarations")) return false;
    std::set<std::string> parameterIds;
    for (auto &b : *ds) parameterIds.insert(b.getAsObject()->getString("bindingId")->str());
    for (auto &x : *bodyLocals) {
      auto *local=x.getAsObject(); auto found=expected.find(local->getString("name")->str());
      if (!need(found!=expected.end(), "source-analysis callable body scope disagrees with declarations")) return false;
      J want=*found->second, actual=*local->get("binding");
      // The source parameters are retained before the function body's synthetic
      // loop remaps their physical IDs. Outer declarations remain exact.
      if (parameterIds.count(want.getAsObject()->getString("bindingId")->str())) {
        want.getAsObject()->erase("hirBindingId");
        actual.getAsObject()->erase("hirBindingId");
      }
      if (!need(want==actual, "source-analysis callable body scope disagrees with declarations")) return false;
    }
    return true;
  }
  const O *sourceSymbol(const O &node) {
    auto *data=node.getObject("originalForm")->getObject("data");
    if (data->getString("tag")=="symbol") return data;
    if (data->getString("tag")=="list") {
      auto *items=data->getArray("items");
      if (items && !items->empty()) {
        auto *head=(*items)[0].getAsObject()->getObject("data");
        if (head->getString("tag")=="symbol") return head;
      }
    }
    return nullptr;
  }
  bool optionalUnits(const J &v) {
    auto *o=v.getAsObject(); auto present=o ? o->getBoolean("present") : std::nullopt;
    if (!need(bool(present), "source-analysis units presence requires boolean")) return false;
    if (!*present) return object(v,{"present"});
    if (!object(v,{"present","value"})) return false;
    auto *units=o->getArray("value"); if (!need(units,"source-analysis units require array")) return false;
    for(auto &x:*units) if(!integer(x)||!need(*x.getAsUINT64()<=65535,"source-analysis UTF16 unit out of range")) return false;
    return true;
  }
  bool optionalCount(const J &v) {
    auto *o=v.getAsObject(); auto present=o ? o->getBoolean("present") : std::nullopt;
    return need(bool(present),"source-analysis count presence requires boolean") &&
      (*present ? object(v,{"present","value"}) && integer(*o->get("value")) : bool(object(v,{"present"})));
  }
  bool globalResolution(const J &v, const O &node, unsigned d) {
    auto *r=object(v,{"tag","global","declaration"});
    if(!r || !enumeration(*r->get("tag"),{"global"})) return false;
    auto *g=object(*r->get("global"),{"phase","namespace","name"});
    auto *head=sourceSymbol(node);
    if(!g || !enumeration(*g->get("phase"),{"runtime","macro"}) || !text(*g->get("namespace")) || !text(*g->get("name"))) return false;
    if(!need(head && head->getString("name")==g->getString("name") && *g->get("phase")==*node.get("phase"),"source-analysis global source name/phase mismatch")) return false;
    auto *decl=r->getObject("declaration"); auto present=decl ? decl->getBoolean("present") : std::nullopt;
    if(!need(bool(present),"source-analysis global declaration requires presence")) return false;
    if(!*present) return object(*r->get("declaration"),{"present"});
    if(!object(*r->get("declaration"),{"present","value"})) return false;
    auto *info=object(*decl->get("value"),{"definitionForm","analysisCompleted","declaration","docstring","origin","initializerForm","initializerPresent","typeFields","once"});
    if(!info || !form(*info->get("definitionForm"),d+1) || !flag(*info->get("analysisCompleted")) || !form(*info->get("declaration"),d+1) || !optionalUnits(*info->get("docstring")) || !optional(*info->get("initializerForm"),d+1) || !flag(*info->get("initializerPresent")) || !optionalCount(*info->get("typeFields")) || !flag(*info->get("once"))) return false;
    auto *symbol=info->getObject("declaration")->getObject("data");
    if(!need(symbol->getString("tag")=="symbol" && symbol->getString("name")==g->getString("name"),"source-analysis global declaration/name mismatch")) return false;
    auto *origin=info->getObject("origin"); auto exists=origin ? origin->getBoolean("present") : std::nullopt;
    if(!need(bool(exists),"source-analysis global origin requires presence")) return false;
    if(!*exists) return object(*info->get("origin"),{"present"});
    if(!object(*info->get("origin"),{"present","value"})) return false;
    auto *value=object(*origin->get("value"),{"sourceBytes","path"});
    return value && integer(*value->get("sourceBytes")) && optionalUnits(*value->get("path"));
  }
  bool node(const J &v, unsigned d) {
    if (!enter(d)) return false;
    auto *o = object(v, {"op", "originalForm", "span", "metadata", "physicalType", "isBody", "context", "phase", "namespace", "scopeNamespace", "snapshotNamespace", "tag", "inferred", "quotedConstTag", "inferredReturn", "resolved", "locals", "declarations", "children", "callable", "captures"});
    if (!o || !enumeration(*o->get("op"), {"let", "closure", "vector", "invoke", "do", "local", "global", "literal"}) || !form(*o->get("originalForm"), d+1) || !span(*o->get("span")) ||
        !forms(*o->get("metadata"), d+1) || !type(*o->get("physicalType")) || !flag(*o->get("isBody")) || !context(*o->get("context")) ||
        !enumeration(*o->get("phase"), {"runtime", "macro"}) || !text(*o->get("namespace")) || !text(*o->get("scopeNamespace")) || !text(*o->get("snapshotNamespace")) ||
        !optional(*o->get("tag"), d+1) || !optional(*o->get("inferred"), d+1) || !optional(*o->get("quotedConstTag"), d+1, true) || !optional(*o->get("inferredReturn"), d+1, true) || !locals(*o->get("locals"), d+1)) return false;
    auto tag = *o->getString("op"); auto *ls = o->getArray("locals");
    auto *resolution=o->get("resolved");
    if (!resolution->getAsNull()) {
      auto *r=resolution->getAsObject(); auto kind=r ? r->getString("tag") : std::nullopt;
      if (!need(bool(kind),"source-analysis resolution requires tag")) return false;
      if (*kind=="local") {
        if(!need(tag=="local" || tag=="invoke","source-analysis resolution on nonlocal node")) return false;
        r=object(*resolution,{"tag","binding"});
        if(!r || !binding(*r->get("binding"),d+1)) return false;
        auto *b=r->getObject("binding"); auto *head=sourceSymbol(*o);
        auto *declaration=b->getObject("declaration")->getObject("data");
        if(!need(visible(*ls,*b->getString("bindingId"),*b->get("hirBindingId")->getAsUINT64()),"source-analysis resolved binding not visible")) return false;
        if(!need(head && head->get("namespace")->getAsNull() && head->getString("name")==declaration->getString("name"),"source-analysis local symbol/resolution mismatch")) return false;
      } else if (*kind=="global") {
        if(!need(tag=="global" || tag=="let" || tag=="closure" || tag=="invoke" || tag=="do","source-analysis resolution on nonlocal node") || !globalResolution(*resolution,*o,d+1)) return false;
      } else return need(false,"source-analysis unsupported resolution tag");
    }
    auto *decls = o->getArray("declarations"); auto *children = o->getArray("children"); auto *caps = o->getArray("captures");
    if (!need(decls && children && caps, "source-analysis declarations/children/captures require arrays")) return false;
    std::set<std::string> declared;
    for (auto &b : *decls) {
      if (!binding(b, d+1)) return false;
      if (!need(declared.insert(b.getAsObject()->getString("bindingId")->str()).second, "source-analysis duplicate source declaration")) return false;
    }
    if (!need(tag=="let" || decls->empty(), "source-analysis declarations on unsupported source node")) return false;
    auto childScope=scopeIndex(*ls);
    for (size_t i=0;i<children->size();++i) {
      auto &x=(*children)[i]; auto *child=x.getAsObject(); auto role=child ? child->getString("role") : std::nullopt;
      if (!need(bool(role), "source-analysis child requires role")) return false;
      if (!object(x, *role=="init" ? std::initializer_list<llvm::StringRef>{"role", "bindingId", "node"} : std::initializer_list<llvm::StringRef>{"role", "node"})) return false;
      if (tag=="let") {
        if (!need(children->size()==decls->size()+1, "source-analysis let child count mismatch")) return false;
        if (i<decls->size()) {
          auto *b=(*decls)[i].getAsObject();
          if (!need(*role=="init" && child->get("bindingId") && *child->get("bindingId")==*b->get("bindingId") && *child->get("node")==*b->get("initializer"), "source-analysis dangling/reordered initializer reference")) return false;
        } else if (!need(*role=="body", "source-analysis let must end in body")) return false;
      } else if (tag=="vector") { if (!need(*role=="item", "source-analysis vector child order/role mismatch")) return false; }
      else if (tag=="invoke") { if (!need(*role==(i==0 ? "callee" : "argument"), "source-analysis invoke child order/role mismatch")) return false; }
      else if (tag=="do") { if (!need(*role==(i+1==children->size() ? "result" : "statement"), "source-analysis do child order/role mismatch")) return false; }
      else return need(false, "source-analysis leaf cannot have executable children");
      if (!node(*child->get("node"), d+1) || !sameScope(*child->getObject("node")->getArray("locals"), childScope)) return false;
      if (tag=="let" && i<decls->size() && !extendScope((*decls)[i], childScope)) return false;
    }
    if (!need((tag!="let" || children->size()==decls->size()+1) && ((tag!="invoke" && tag!="do") || !children->empty()), "source-analysis missing source children")) return false;
    if (tag=="closure") {
      if (!need(!o->get("callable")->getAsNull(), "source-analysis closure requires callable") || !callable(*o->get("callable"), d)) return false;
      if (!sameScope(*o->getObject("callable")->getArray("entryLocals"), scopeIndex(*ls))) return false;
      auto *physical=o->getObject("physicalType");
      if (!need(physical->getString("tag")=="closure" && physical->get("arity") && physical->get("arity")->getAsUINT64()==o->getObject("callable")->getArray("parameters")->size(), "source-analysis closure arity disagrees with callable")) return false;
    } else if (!need(bool(o->get("callable")->getAsNull()) && caps->empty(), "source-analysis callable/captures on nonclosure")) return false;
    std::set<std::string> seenCaptures;
    for (auto &c : *caps) {
      auto *b=object(c, {"bindingId", "hirBindingId"}); if (!b || !identity(*b->get("bindingId")) || !integer(*b->get("hirBindingId"))) return false;
      auto id=*b->getString("bindingId");
      if (!need(seenCaptures.insert(id.str()).second && visible(*ls, id, *b->get("hirBindingId")->getAsUINT64()), "source-analysis dangling/duplicate capture reference")) return false;
    }
    if(tag=="invoke" && !resolution->getAsNull()) {
      auto *callee=(*children)[0].getAsObject()->getObject("node");
      if(!need(*callee->get("resolved")==*resolution,"source-analysis invoke head/callee resolution mismatch")) return false;
    }
    if(tag=="global") {
      auto *data=o->getObject("originalForm")->getObject("data");
      return need(!resolution->getAsNull() && resolution->getAsObject()->getString("tag")=="global" && data->getString("tag")=="symbol","source-analysis global requires symbol/global resolution");
    }
    if (tag=="local") {
      if (!need(!o->get("resolved")->getAsNull(), "source-analysis local requires resolved declaration")) return false;
      auto *source=o->getObject("originalForm")->getObject("data");
      auto *declaration=o->getObject("resolved")->getObject("binding")->getObject("declaration")->getObject("data");
      return need(source->getString("tag")=="symbol" && source->get("namespace") && source->get("namespace")->getAsNull() && source->getString("name")==declaration->getString("name"), "source-analysis local symbol/resolution mismatch");
    }
    if (tag=="literal") {
      auto *data=o->getObject("originalForm")->getObject("data");
      return enumeration(*data->get("tag"), {"nil", "bool", "f64", "utf16"});
    }
    return true;
  }
  bool envelope(const J &v) {
    auto *o=object(v, {"schema", "selectedFacts"});
    return o && need(o->getString("schema")=="suss.source-analysis.draft.v1", "source-analysis unsupported schema version") && node(*o->get("selectedFacts"), 0);
  }
};
inline mlir::LogicalResult validate(mlir::Operation *op, mlir::NamedAttribute attr) {
  auto name=attr.getName().strref();
  if (name!="suss.analysis" && name!="suss.source_analysis") return mlir::success();
  auto payload=mlir::dyn_cast<mlir::StringAttr>(attr.getValue());
  if (!payload) return op->emitOpError("source-analysis requires versioned JSON string; legacy dictionaries unsupported");
  auto text=payload.getValue();
  if (text.size()>1048576) return op->emitOpError("source-analysis exceeds 1 MiB");
  if (!UniqueKeys(text).check()) return op->emitOpError("source-analysis invalid JSON, duplicate keys, or nesting limit");
  auto parsed=llvm::json::parse(text);
  if (!parsed) { llvm::consumeError(parsed.takeError()); return op->emitOpError("source-analysis invalid JSON"); }
  Validator validator;
  if (!validator.envelope(*parsed)) return op->emitOpError(validator.error);
  if (name=="suss.source_analysis") {
    if (op->getName().getStringRef()!="builtin.module") return op->emitOpError("source-analysis sidecar requires module attachment");
    return mlir::success();
  }
  auto *facts=parsed->getAsObject()->getObject("selectedFacts");
  auto tag=facts->getString("op"); auto opName=op->getName().getStringRef();
  if (opName=="suss.const" && tag=="literal" && facts->getObject("originalForm")->getObject("data")->getString("tag")=="f64") return mlir::success();
  if (opName=="suss.closure" && tag=="closure") return mlir::success();
  return op->emitOpError("source-analysis unsupported operation/source attachment mapping");
}
} // namespace suss::analysis
