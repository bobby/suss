(ns graph.app
  (:require graph.empty [graph.lib :as g :refer [one two] :rename {one renamed}] [graph.lib :refer [one] :rename {one one}] [graph.lib :refer [one]])
  (:require-macros graph.macempty [graph.tools :as m :refer [one two] :rename {one renamed-macro}] [graph.tools :refer [one] :rename {one one}] [graph.tools :refer [one]]))
(def observed (m/observe))
(defn -main [] (println (.stringify js/JSON observed)))
(set! *main-cli-fn* -main)
