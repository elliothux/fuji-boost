import { createSignal, For } from "solid-js";
import { FocusScope, Text, View } from "@pocketjs/framework/components";
import { onFrame } from "@pocketjs/framework/lifecycle";
import { reportAppAction } from "@pocketjs/framework/host";

/** M4 workload: a recipe list (UP/DOWN focus, OK selects), its detail panel,
 *  a once-per-second uptime label and an activity strip that changes every
 *  frame, so every frame runs JS, layout and raster work. */
interface Recipe {
  name: string;
  simulation: string;
  grain: string;
  tone: string;
  wb: string;
}

const RECIPES: Recipe[] = [
  { name: "Kodachrome 64", simulation: "Classic Chrome", grain: "Weak / Small", tone: "H -1  S +1", wb: "Daylight R+2 B-5" },
  { name: "Portra 400", simulation: "Classic Neg", grain: "Strong / Small", tone: "H -1  S -1", wb: "5200K R+1 B-4" },
  { name: "Tri-X 400", simulation: "Acros + Y", grain: "Strong / Large", tone: "H +2  S +1", wb: "Auto" },
  { name: "Superia 800", simulation: "Classic Neg", grain: "Strong / Large", tone: "H 0  S +2", wb: "Auto R+2 B-1" },
  { name: "Eterna Cinema", simulation: "Eterna", grain: "Off", tone: "H -2  S -2", wb: "Auto R-1 B+1" },
];

const STRIP = 12;
const FRAMES_PER_SECOND = 30;

export default function Dashboard() {
  const [selected, setSelected] = createSignal(0);
  const [frame, setFrame] = createSignal(0);
  onFrame(() => setFrame(frame() + 1));
  const seconds = () => Math.floor(frame() / FRAMES_PER_SECOND);
  const active = () => Math.floor(frame() / 2) % STRIP;
  const recipe = () => RECIPES[selected()];

  return (
    <FocusScope class="w-full h-full flex-col p-4 gap-3 bg-zinc-950">
      <View class="flex-row items-center justify-between">
        <View class="flex-col">
          <Text class="text-xs text-emerald-400 tracking-wide">FUJI BOOST · CUSTOM FIRMWARE</Text>
          <Text class="text-lg text-white font-bold">Film Recipes</Text>
        </View>
        <Text class="text-sm text-zinc-400">{`up ${seconds()}s`}</Text>
      </View>
      <View class="flex-row gap-3 grow">
        <View class="flex-col gap-1 w-[190]">
          <For each={RECIPES}>
            {(item, index) => (
              <View
                focusable
                onPress={() => {
                  setSelected(index());
                  reportAppAction("recipe", index());
                }}
                class={
                  selected() === index()
                    ? "px-3 py-1 rounded-lg bg-emerald-700 focus:bg-emerald-500"
                    : "px-3 py-1 rounded-lg bg-zinc-800 focus:bg-zinc-600"
                }
              >
                <Text class="text-sm text-white">{item.name}</Text>
              </View>
            )}
          </For>
        </View>
        <View class="flex-col gap-2 grow p-3 rounded-xl bg-zinc-900">
          <View class="grow rounded-lg bg-[#010008]" />
          <Text class="text-base text-amber-300 font-bold">{recipe().name}</Text>
          <Text class="text-sm text-zinc-300">{`Film  ${recipe().simulation}`}</Text>
          <Text class="text-sm text-zinc-300">{`Grain  ${recipe().grain}`}</Text>
          <Text class="text-sm text-zinc-300">{`Tone  ${recipe().tone}`}</Text>
          <Text class="text-sm text-zinc-300">{`WB  ${recipe().wb}`}</Text>
        </View>
      </View>
      <View class="flex-row gap-1 justify-center">
        <For each={Array.from({ length: STRIP }, (_, i) => i)}>
          {(i) => <View class={active() === i ? "w-6 h-2 rounded-full bg-emerald-400" : "w-6 h-2 rounded-full bg-zinc-700"} />}
        </For>
      </View>
    </FocusScope>
  );
}
