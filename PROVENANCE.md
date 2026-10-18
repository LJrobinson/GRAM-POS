# Provenance and licensing status

This milestone is a new implementation for LJ Robinson's configurable open-source register project. It does not vendor the existing UnitMath, CannabisMath, or MOBY implementations. Those projects informed the separation of physical units, nominal retail sizes, exact calculations, and audit-friendly outputs.

## Referenced work

- UnitMath: https://github.com/LJrobinson/UnitMath — unit helpers and quantity modelling reviewed as background. Existing floating-point implementation was not copied into the pricing kernel.
- CannabisMath, revision `f36238294fa5a8c9005b085fe2273a67a9ddf4aa`: https://github.com/LJrobinson/CannabisMath/tree/f36238294fa5a8c9005b085fe2273a67a9ddf4aa — decimal/rounding behavior and the distinction between physical and retail ounces informed the design.
- Its shared fixtures: https://github.com/LJrobinson/CannabisMath/blob/f36238294fa5a8c9005b085fe2273a67a9ddf4aa/shared/test-fixtures/core-fixtures.json — selected simple price-per-gram examples are adapted into acceptance tests. This is not a claim of full cross-language parity. Potency, dosing, cash-payment rounding increments, and other unrelated calculations are outside this milestone.

## License decision pending

The owner intends this project to be open source, but has not selected its license. No license is selected on their behalf in this package. Before public distribution or accepting contributions, choose a license, add the appropriate license/copyright notices, and verify notices for any code or fixtures actually incorporated. A repository's public visibility or a manifest license declaration is not a complete provenance review.

No existing repository was modified. The owner authorized a new public `gram-pos` GitHub repository. Publication status is verified separately at delivery. No changes were authorized to existing repositories.
