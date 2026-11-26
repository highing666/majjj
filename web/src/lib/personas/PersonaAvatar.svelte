<script lang="ts">
  import personaAtlasPngUrl from "../../assets/atlas/personas@2x.png";
  import type { PersonaId } from "../protocol/generated/PersonaId";
  import type { PersonaMood } from "../protocol/generated/PersonaMood";
  import { getPersonaMeta } from "../protocol/personas";
  import { projectPersonaAvatar } from "./personaAtlas";

  interface Props {
    personaId: PersonaId;
    mood?: PersonaMood | null;
    size?: number;
    rounded?: "md" | "full";
  }

  let {
    personaId,
    mood = null,
    size = 44,
    rounded = "md",
  }: Props = $props();

  const meta = $derived(getPersonaMeta(personaId));
  const projection = $derived(projectPersonaAvatar(personaId, mood, size));
</script>

<span
  class="persona-avatar"
  class:persona-avatar-round={rounded === "full"}
  role="img"
  aria-label={projection.ariaLabel}
  title={projection.ariaLabel}
  style="
    width: {projection.size}px;
    height: {projection.size}px;
    background-image: url({personaAtlasPngUrl});
    background-size: {projection.backgroundSize};
    background-position: {projection.backgroundPosition};
    border-color: {meta.accentHex};
  "
></span>

<style>
  .persona-avatar {
    display: inline-block;
    flex: 0 0 auto;
    background-repeat: no-repeat;
    border-radius: 10px;
    border: 1.5px solid #181a20;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.28);
    overflow: hidden;
  }

  .persona-avatar-round {
    border-radius: 999px;
  }
</style>
