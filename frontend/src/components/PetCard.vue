<script setup lang="ts">
import { computed } from 'vue'
import SpeciesMark from './SpeciesMark.vue'
import { api } from '../api'
import { petGradient, ageFrom, dueLabel, isOverdue, petLifeStage } from '../format'
import type { PetSummary } from '../types'

const props = defineProps<{ pet: PetSummary }>()
const photoUrl = computed(() => api.petPhotoUrl(props.pet))
const age = computed(() => ageFrom(props.pet.birth_date))
const stage = computed(() => petLifeStage(props.pet)) // #73
// "What's next" replaced the care-score pill (#35): a neutral fact — the pet's
// soonest reminder, or a calm "all caught up" — never a grade of the owner.
const next = computed(() => props.pet.next_reminder)
const overdue = computed(() => isOverdue(next.value?.due_on))
</script>

<template>
  <router-link :to="`/pets/${pet.id}`" class="pcard" data-testid="pet-card" :data-pet="pet.id">
    <div class="portrait" :style="{ background: petGradient(pet) }">
      <img v-if="photoUrl" class="pphoto" :src="photoUrl" :alt="pet.name" />
      <span class="badge">{{ pet.species }}</span>
      <template v-if="!photoUrl">
        <span class="init">{{ pet.name[0] }}</span>
        <SpeciesMark :species="pet.species" class="paw" />
      </template>
    </div>
    <div class="pcard-body">
      <h3>{{ pet.name }}</h3>
      <div class="meta">{{ pet.breed || pet.species }}<span v-if="age"> · {{ age }}</span><span v-if="stage" class="stage-chip" data-testid="pet-card-stage">{{ stage }}</span></div>
      <div class="pcard-status">
        <span v-if="next" class="pill" :class="{ over: overdue }" data-testid="pet-card-next">
          <span class="d"></span>{{ next.title }} · {{ dueLabel(next.due_on) }}
        </span>
        <span v-else class="pill ok" data-testid="pet-card-next">
          <span class="d"></span>All caught up
        </span>
      </div>
    </div>
  </router-link>
</template>
