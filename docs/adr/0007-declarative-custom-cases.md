# Declarative Custom Cases over built-in primitives

Users will extend Ronchi by creating portable YAML Custom Cases from versioned built-in Fault Primitives, bounded conditions, sequencing, and evidence-linked Finding rules. The same artifact may be authored as a file, through the local dashboard, or by a Coding Agent. Ronchi will not expose an arbitrary executable plugin mechanism for user-defined fault semantics. When a required behavior cannot be expressed by the installed primitive catalog, the primitive must be added to the maintained Ronchi core through a release or accepted contribution.

## Considered Options

- Arbitrary in-process plugins were rejected because they weaken safety, reproducibility, evidence semantics, portability, and compatibility for human and Coding Agent workflows.
- Forking the core for every custom behavior was rejected because Custom Cases should be shareable and runnable by an unchanged Ronchi installation when the required primitives exist.
- Built-in Cases only were rejected because users need to reproduce project-specific incident sequences and conditions without waiting for a new curated Case.

## Consequences

- The Fault Primitive catalog is a first-class, versioned product surface.
- Custom Cases are inspectable before execution and contain no arbitrary executable fault code.
- Built-in and Custom Cases use the same execution and evidence model, but their provenance and Coverage Claims remain visibly distinct.
- Ronchi maintainers retain responsibility for implementing and validating new protocol semantics.
- YAML is the declarative representation; its exact schema and authoring details remain open decisions.
