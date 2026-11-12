#!/usr/bin/env bb
(require '[clojure.string :as str] '[babashka.fs :as fs])

(defn normalize [text]
  (->> (str/split-lines text) (map str/trim) (remove str/blank?) (str/join "\n")))

(defn open-ended [normalized]
  (->> (str/split-lines normalized) reverse (drop-while #{"}"}) reverse (str/join "\n")))

(defn rust-blocks [text]
  (map second (re-seq #"(?s)```rust\n(.*?)```" text)))

(defn cited-tests [text]
  (->> (re-seq #"`([a-z][a-z0-9]*(?:_[a-z0-9]+){4,})`" text) (map second) distinct))

(defn cited-mutants [text]
  (->> (re-seq #"mutants? ((?:[CTGH]\d+)(?:(?:, | and | to )(?:[CTGH]\d+))*)" text)
       (mapcat #(re-seq #"[CTGH]\d+" (second %)))
       distinct))

(let [workspace (fs/parent *file*)
      guide (fs/path workspace "docs" "architecture")
      docs (sort (fs/glob guide "*.req.md"))
      sources (->> (fs/glob workspace "*/src/**.rs") (map (comp normalize slurp str)))
      rust (->> (fs/glob workspace "*/{src,tests}/**.rs") (map (comp slurp str)) (str/join "\n"))
      planted (set (map second (re-seq #"\(\"([CTGH]\d+) " (slurp (str (fs/path workspace "mutants.py"))))))
      missing (for [doc docs
                    [index block] (map-indexed vector (rust-blocks (slurp (str doc))))
                    :when (not-any? #(str/includes? % (open-ended (normalize block))) sources)]
                (str (fs/file-name doc) " rust block " (inc index) ": " (first (str/split-lines block))))
      unknown (for [doc docs
                    :let [text (slurp (str doc))]
                    problem (concat
                             (for [test (cited-tests text) :when (not (str/includes? rust (str "fn " test "(")))]
                               (str "names a test that does not exist: " test))
                             (for [mutant (cited-mutants text) :when (not (planted mutant))]
                               (str "names a mutant that mutants.py does not plant: " mutant)))]
                (str (fs/file-name doc) " " problem))
      total (count (mapcat (comp rust-blocks slurp str) docs))]
  (run! println missing)
  (run! println unknown)
  (println (str total " rust blocks, " (count missing) " not found in the workspace; " (count unknown) " cited names unknown"))
  (when (seq missing)
    (println "arch-ajd: a Rust block in docs/architecture must be code in the workspace, verbatim.")
    (println "Fix: copy the changed code into the block, or replace the block with a pointer to the file."))
  (when (seq unknown)
    (println "arch-7iz: a criterion must name a test or a mutant that exists.")
    (println "Fix: rename the citation to the test or mutant as it is now called."))
  (System/exit (if (or (zero? total) (seq missing) (seq unknown)) 1 0)))
