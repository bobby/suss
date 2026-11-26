(prn [
(do (eval '(defmacro effects-maker [] (eval '(def effects-created 41)) 42)) (eval '[(effects-maker) effects-created]))
(do (eval '(defmacro effects-maker-fail [] (eval '(def effects-prefix 41)) 42)) [(try (eval '[(effects-maker-fail) effects-unresolved]) :accepted (catch Throwable e :rejected)) (eval 'effects-prefix)])
(do (eval '(defmacro effects-maker-outer [] (eval '(def effects-from-outer 41)) 42)) (eval '(def effects-outer (do (effects-maker-outer) effects-from-outer))) (eval 'effects-outer))
(do (eval '(def effects-selected 10)) (eval '(defmacro effects-change-selected [] (eval '(def effects-selected 20)) 42)) (eval '(def effects-selected (do (effects-change-selected) effects-selected))) (eval 'effects-selected))
(do (eval '(defmacro effects-make-macro [] (eval '(defmacro effects-created-macro [] 43)) 42)) (eval '[(effects-make-macro) (effects-created-macro)]))
(do (eval '(def ^:macro effects-manual (fn [form env x] x))) [(eval '(effects-manual 42)) (.isMacro (eval '(var effects-manual))) (:macro (meta (eval '(var effects-manual))))])
(do (eval '(def ^{:macro :enabled} effects-manual-truthy (fn [form env x] x))) [(eval '(effects-manual-truthy 43)) (.isMacro (eval '(var effects-manual-truthy))) (:macro (meta (eval '(var effects-manual-truthy))))])
]
)
