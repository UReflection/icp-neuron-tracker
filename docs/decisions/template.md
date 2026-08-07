# Decision Record NNN: [Title]

**Reference:** DR-NNN  
**Status:** [Proposed | Accepted | Deprecated | Superseded by DR-XXX]  
**Date Proposed:** YYYY-MM-DD  
**Date Decided:** YYYY-MM-DD  
**Deciders:** [List of people involved]  
**Technical Story:** [Link to issue or discussion]

---

## Context

What is the situation requiring a decision?

Describe the forces at play:
- Business requirements driving this
- Technical constraints we face
- Timeline or resource limitations
- Dependencies on other systems
- Current pain points being addressed

Be specific. Future readers need to understand why this mattered.

---

## Decision

What are we choosing to do?

State the decision clearly and unambiguously. Someone reading this should know exactly what was decided.

Example: "We will use SQLite for local data persistence with Refinery for schema migrations."

Not: "We might consider using a database for storing things."

---

## Consequences

What happens because of this decision?

### Positive Consequences

What becomes easier, better, faster, safer?

- Specific improvements to development workflow
- Performance gains
- Reduced complexity in certain areas
- Better alignment with principles
- Cost savings or resource efficiency

### Negative Consequences

What becomes harder, slower, more complex?

- New constraints introduced
- Technical debt accepted
- Learning curve for team
- Future flexibility reduced
- Additional dependencies

### Neutral Consequences

What changes without clear positive or negative impact?

- Different workflows without quality change
- Shifted complexity from one area to another
- Trade-offs that balance out

Be honest. Every decision has trade-offs.

---

## Alternatives Considered

What other options did we evaluate?

### Alternative 1: [Name]

**Description:**  
Brief explanation of this alternative approach.

**Pros:**
- Advantage 1
- Advantage 2
- Advantage 3

**Cons:**
- Disadvantage 1
- Disadvantage 2
- Disadvantage 3

**Why Not Chosen:**  
Specific reason this option was rejected. What was the deciding factor?

### Alternative 2: [Name]

**Description:**  
Brief explanation of this alternative approach.

**Pros:**
- Advantage 1
- Advantage 2

**Cons:**
- Disadvantage 1
- Disadvantage 2

**Why Not Chosen:**  
Specific reason this option was rejected.

### Alternative 3: [Name]

Continue pattern as needed...

---

## Implementation Notes

How do we implement this decision?

**Affected Components:**
- List of files, modules, or systems that change
- New files or directories to create
- Existing code to refactor

**Migration Path:**
- If changing existing system, how do we transition?
- Data migration requirements
- Backward compatibility considerations
- Rollout strategy

**Key Implementation Details:**
- Critical configuration settings
- Important dependencies to add
- Performance considerations
- Security implications

---

## Validation

How do we know this decision is working?

**Success Metrics:**
- Measurable outcomes that indicate success
- Performance benchmarks
- Developer productivity improvements
- User experience gains

**Review Date:**  
When should we revisit this decision? Format: YYYY-MM-DD

---

## References

- [Link to related discussion]
- [Link to related ADRs]
- [External documentation]
- [Research papers or articles]
- [Proof of concepts or spikes]

---

## Revision History

| Date | Author | Change |
|------|--------|--------|
| YYYY-MM-DD | Name | Initial draft |
| YYYY-MM-DD | Name | Updated after review |
| YYYY-MM-DD | Name | Marked as accepted |

---

**U Reflection Design & Build Inc.**

Every decision reflects our understanding of reality at a point in time.  
Preserve the reasoning. Learn from the past.