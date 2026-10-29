use super::*;

fn names(ps: &[Part]) -> Vec<String> {
    ps.iter()
        .map(|p| format!("{}:{}", p.kind, p.name))
        .collect()
}

const SETUP: &str = r#"<template>
  <div>
    <OrderRow v-for="o in orders" :key="o.id" />
    <base-button @click="save">Save</base-button>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
interface Props { orderId: string; compact?: boolean }
const props = withDefaults(defineProps<Props>(), { compact: false })
const emit = defineEmits<{ (e: 'saved', id: string): void; (e: 'close'): void }>()
const model = defineModel<string>('query')
const open = ref(false)
const { orders, total } = storeToRefs(useOrdersStore())
const { t } = useI18n()
const label = computed(() => t('x'))
watch(open, () => {})
onMounted(() => {})
function save() { emit('saved', props.orderId) }
const close = () => emit('close')
</script>
"#;

#[test]
fn a_script_setup_component() {
    let a = &read("src/OrderList.vue", SETUP, Some(".vue"))[0];
    assert_eq!(
        (a.framework.as_str(), a.name.as_str()),
        ("vue", "OrderList")
    );
    assert_eq!(
        names(&a.inputs),
        [
            "prop:orderId",
            "prop:compact?",
            "emit:saved",
            "emit:close",
            "model:query"
        ]
    );
    assert_eq!(a.inputs[0].detail.as_deref(), Some("string"));
    assert_eq!(a.inputs[2].detail.as_deref(), Some("id: string"));
    assert_eq!(
        names(&a.state),
        [
            "ref:open",
            "storeToRefs:orders",
            "storeToRefs:total",
            "computed:label",
            "watch:open"
        ]
    );
    assert_eq!(names(&a.uses), ["composable:useI18n"]);
    assert_eq!(
        names(&a.functions),
        ["lifecycle:onMounted", "function:save", "function:close"]
    );
    assert_eq!(names(&a.renders), ["tag:OrderRow", "tag:BaseButton"]);
    assert_eq!(a.renders[0].line, 3);
    let t = a.template.unwrap();
    assert_eq!((t.start_line, t.end_line), (1, 6));
    assert_eq!(a.state[0].line, 14);
}

const OPTIONS: &str = r#"<template><Card /></template>
<script>
import { mapGetters, mapActions } from 'vuex'
export default defineComponent({
  props: { order: Object, compact: Boolean },
  emits: ['close'],
  data() { return { open: false, page: 1 } },
  computed: { ...mapGetters('cart', ['items']), total() { return 1 } },
  watch: { page() {} },
  methods: { ...mapActions(['load']), save() {} },
  mounted() {},
  setup() { const q = ref(''); return { q } },
})
</script>
"#;

#[test]
fn an_options_api_component() {
    let a = &read("Cart.vue", OPTIONS, Some(".vue"))[0];
    assert_eq!(
        names(&a.inputs),
        ["prop:order", "prop:compact", "emit:close"]
    );
    assert_eq!(a.inputs[0].detail.as_deref(), Some("Object"));
    assert_eq!(
        names(&a.state),
        [
            "data:open",
            "data:page",
            "computed:total",
            "watch:page",
            "ref:q"
        ]
    );
    assert_eq!(names(&a.uses), ["store:mapGetters", "store:mapActions"]);
    assert_eq!(a.uses[0].detail.as_deref(), Some("'cart', ['items']"));
    assert_eq!(names(&a.functions), ["method:save", "lifecycle:mounted"]);
    assert_eq!(names(&a.renders), ["tag:Card"]);
}

const REACT: &str = r#"import { useState, useEffect, memo } from 'react'
type Props = { id: string; onPick: (id: string) => void }
export function OrderCard({ id, onPick }: Props) {
  const [open, setOpen] = useState(false)
  const order = useOrder(id)
  useEffect(() => {}, [id])
  const pick = () => onPick(id)
  return <Card title={id}><Badge.Dot />{open && <Details order={order} />}</Card>
}
export const Row = memo((props: Props) => <li>{props.id}</li>)
export default function () { return <><Row id="a" onPick={() => {}} /></> }
function helper() { return 1 }
const lower = () => <div />
"#;

#[test]
fn react_function_components() {
    let all = read("src/OrderCard.tsx", REACT, None);
    let got: Vec<&str> = all.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(got, ["OrderCard", "Row", "OrderCard"]);
    let a = &all[0];
    assert_eq!(a.framework, "react");
    assert_eq!(names(&a.inputs), ["prop:id", "prop:onPick"]);
    assert_eq!(names(&a.state), ["useState:open", "effect:useEffect"]);
    assert_eq!(names(&a.uses), ["hook:useOrder"]);
    assert_eq!(a.uses[0].detail.as_deref(), Some("order"));
    assert_eq!(names(&a.functions), ["function:pick"]);
    assert_eq!(
        names(&a.renders),
        ["tag:Card", "tag:Badge.Dot", "tag:Details"]
    );
    assert_eq!((a.range.start_line, a.range.end_line), (3, 9));
    // a props type declared in the file
    assert_eq!(names(&all[1].inputs), ["prop:id", "prop:onPick"]);
    assert_eq!(
        all[1].inputs[1].detail.as_deref(),
        Some("(id: string) => void")
    );
    assert!(read("src/a.ts", REACT, None).is_empty());
}
