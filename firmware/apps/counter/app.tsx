import { createSignal } from "solid-js";
import { FocusScope, Text, View } from "@pocketjs/framework/solid/components";
import { reportAppAction } from "@pocketjs/framework/host";

/** M3 smoke app: OK or a tap on +1 increments, the second button resets. */
export default function Counter() {
  const [count, setCount] = createSignal(0);
  const update = (next: number) => {
    setCount(next);
    reportAppAction("count", next);
  };
  return (
    <FocusScope class="w-full h-full flex-col justify-between p-5 bg-slate-900">
      <View class="flex-row items-center justify-between">
        <Text class="text-lg text-white font-bold tracking-wide">FUJI BOOST</Text>
        <Text class="text-xs text-slate-400 tracking-wide">X-T4 · POCKETJS</Text>
      </View>
      <View class="w-full grow rounded-lg bg-[#010008]" />
      <View class="flex-col items-center gap-1">
        <Text class="text-5xl text-amber-400 font-bold">{count()}</Text>
        <Text class="text-sm text-slate-400">presses</Text>
      </View>
      <View class="flex-row justify-center gap-4">
        <View
          focusable
          onPress={() => update(count() + 1)}
          class="px-5 py-2 rounded-lg bg-blue-700 focus:bg-blue-500 active:bg-blue-800"
        >
          <Text class="text-base text-white font-bold">+1</Text>
        </View>
        <View
          focusable
          onPress={() => update(0)}
          class="px-5 py-2 rounded-lg bg-slate-700 focus:bg-slate-500 active:bg-slate-800"
        >
          <Text class="text-base text-white font-bold">Reset</Text>
        </View>
      </View>
    </FocusScope>
  );
}
