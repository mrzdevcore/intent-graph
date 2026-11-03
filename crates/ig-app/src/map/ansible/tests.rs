use super::*;
use crate::map::{ArrowKind, arrows};
use crate::testing::{Fake, env};
use ig_core::testing::MemFiles;

const DEPLOY: &str = "---
- name: Deploy payment API
  include_role:
    name: helper_deploy_helm_chart
  vars:
    values: \"{{ lookup('template', 'values.yml.j2') | from_yaml }}\"
";

const VALUES: &str = "application:
  configmap:
    data:
      PRODUCT_API_HOST: \"{{ api_url }}\"
      NOTIFICATIONS_API_HOST: \"{{ api_url }}\"
      AUTH0_HOST: \"{{ auth0_url }}\"
      DATABASE_SCHEMA: payment
      LOGGER_LEVEL: info
";

fn repo() -> MemFiles {
    MemFiles::with(&[
        ("ansible/deploy.yml", "- hosts: all\n"),
        ("ansible/roles/payment_api/tasks/main.yml", DEPLOY),
        (
            "ansible/roles/payment_api/meta/targets.yml",
            "---\n- retail\n",
        ),
        ("ansible/roles/payment_api/templates/values.yml.j2", VALUES),
        (
            "ansible/roles/payment_api/defaults/main.yml",
            "payment_api_replicas: 2\n",
        ),
        (
            "ansible/roles/product_api/tasks/main.yml",
            "- name: Deploy\n  debug: {}\n",
        ),
        ("ansible/roles/product_api/meta/targets.yml", "- retail\n"),
        (
            "ansible/roles/notification_api/tasks/main.yml",
            "- name: Deploy\n  debug: {}\n",
        ),
        (
            "ansible/roles/notification_api/meta/targets.yml",
            "- retail\n",
        ),
        (
            "ansible/roles/front_shop/tasks/main.yml",
            "- name: Deploy\n  debug: {}\n",
        ),
        ("ansible/roles/front_shop/meta/targets.yml", "- aladin\n"),
        (
            "ansible/roles/helper_deploy_helm_chart/tasks/main.yml",
            "- name: Helm\n  debug: {}\n",
        ),
    ])
}

#[test]
fn roles_are_services_grouped_by_where_they_deploy() {
    let a = read(&repo()).unwrap();
    let got: Vec<(&str, Level, BoxKind, &str)> = (a.boxes.iter())
        .map(|b| (b.id.as_str(), b.level, b.guess.kind, b.guess.title.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            // one role alone for its target: still its own container, the only small one
            ("aladin", Level::Container, BoxKind::Group, "Aladin"),
            (
                "aladin/front-shop",
                Level::Component,
                BoxKind::App,
                "front_shop"
            ),
            ("auth0", Level::Container, BoxKind::External, "Auth0"),
            ("database", Level::Container, BoxKind::Store, "Database"),
            ("helpers", Level::Container, BoxKind::Group, "Helpers"),
            (
                "helpers/helper-deploy-helm-chart",
                Level::Component,
                BoxKind::Module,
                "helper_deploy_helm_chart"
            ),
            ("retail", Level::Container, BoxKind::Group, "Retail"),
            (
                "retail/notification-api",
                Level::Component,
                BoxKind::Service,
                "notification_api"
            ),
            (
                "retail/payment-api",
                Level::Component,
                BoxKind::Service,
                "payment_api"
            ),
            (
                "retail/product-api",
                Level::Component,
                BoxKind::Service,
                "product_api"
            ),
            ("system", Level::System, BoxKind::System, "ansible"),
        ]
    );
}

#[test]
fn tasks_templates_and_settings_draw_the_arrows() {
    let a = read(&repo()).unwrap();
    let lifted = arrows(&a.boxes, &a.graph);
    let has = |from: &str, to: &str, k: ArrowKind| {
        lifted
            .iter()
            .any(|x| x.from == from && x.to == to && x.kinds.contains(&k))
    };
    assert!(
        has("retail/payment-api", "retail/product-api", ArrowKind::Calls),
        "{lifted:#?}"
    );
    // NOTIFICATIONS_API_HOST names notification_api
    assert!(has(
        "retail/payment-api",
        "retail/notification-api",
        ArrowKind::Calls
    ));
    assert!(has("retail", "helpers", ArrowKind::Calls));
    assert!(has("retail", "auth0", ArrowKind::Calls));
    assert!(has("retail", "database", ArrowKind::Reads));
    let task = (a.graph.nodes.iter())
        .find(|n| n.kind == "task" && n.file.contains("payment_api"))
        .unwrap();
    assert_eq!(
        (
            task.name.as_str(),
            task.range.start_line,
            task.range.end_line
        ),
        ("Deploy payment API", 2, 6)
    );
}

#[test]
fn ig_view_draws_an_ansible_tree_down_to_its_tasks() {
    let files = repo();
    let v = crate::map::view::run(&env(&files, None, &Fake)).unwrap();
    let task = (v.boxes.iter())
        .find(|b| b.kind.value == BoxKind::Task && b.title.value == "Deploy payment API")
        .unwrap();
    assert_eq!(task.parent.as_deref(), Some("retail/payment-api"));
    let tpl = "ansible/roles/payment_api/templates/values.yml.j2#file";
    // a role's text can hold secrets: its boxes, never its text in the page
    assert!(v.boxes.iter().any(|b| b.id == tpl));
    assert!(v.sources.is_empty(), "{:?}", v.sources.keys());
    assert!(
        v.notices.iter().all(|n| !n.contains("not written")),
        "{:?}",
        v.notices
    );
}

#[test]
fn a_large_tree_keeps_every_box_to_twelve() {
    let systems = [
        "AUTH0",
        "SENTRY",
        "STRIPE",
        "ALGOLIA",
        "CONTENTFUL",
        "IZANAMI",
        "DALENYS",
        "MANGOPAY",
        "ZENDESK",
        "MAILJET",
        "MIRAKL",
        "LANGFUSE",
        "REDIS",
        "MONGO",
        "KAFKA",
    ];
    let mut owned: Vec<(String, String)> = vec![];
    for i in 0..30 {
        let r = format!("ansible/roles/svc{i:02}_api");
        owned.push((
            format!("{r}/tasks/main.yml"),
            "- name: Deploy\n  debug: {}\n".into(),
        ));
        let t = if i < 20 {
            "common"
        } else {
            ["aladin", "farmi", "retail"][i % 3]
        };
        owned.push((format!("{r}/meta/targets.yml"), format!("- {t}\n")));
        let s = systems[i % systems.len()];
        owned.push((
            format!("{r}/templates/values.yml.j2"),
            format!("{s}_HOST: x\n"),
        ));
    }
    let refs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let files = MemFiles::with(&refs);
    let v = crate::map::view::run(&env(&files, None, &Fake)).unwrap();
    let mut kids: BTreeMap<&str, usize> = BTreeMap::new();
    for b in &v.boxes {
        *kids.entry(b.parent.as_deref().unwrap_or("-")).or_default() += 1;
    }
    assert!(kids.iter().all(|(p, n)| *p == "-" || *n <= MAX), "{kids:?}");
    assert!(
        v.notices.iter().all(|n| !n.contains("not written")),
        "{:?}",
        v.notices
    );
    let titles: Vec<&str> = (v.boxes.iter())
        .filter(|b| b.parent.as_deref() == Some("common"))
        .map(|b| b.title.value.as_str())
        .collect();
    assert_eq!(titles, ["APIs 1/2 (10)", "APIs 2/2 (10)"]);
}

/// A homelab's shape: roles nested in a folder, deployed by a playbook, settings as `KEY=value`.
fn homelab() -> MemFiles {
    let task = "- name: Deploy\n  debug: {}\n";
    MemFiles::with(&[
        (
            "ansible/playbooks/site.yml",
            "---\n- hosts: gitlab\n  become: true\n  roles:\n    - services/gitlab\n\n\
             - hosts: postgresql\n  roles:\n    - role: services/postgresql\n      tags: db\n\n\
             - hosts: firefly\n  roles:\n    - { role: services/firefly-iii }\n    - services/firefly-importer\n\n\
             - hosts: all\n  roles:\n    - common\n",
        ),
        (
            "ansible/inventory/hosts.yml",
            "all:\n  hosts:\n    gitlab:\n",
        ),
        ("ansible/roles/common/tasks/main.yml", task),
        ("ansible/roles/services/gitlab/tasks/main.yml", task),
        (
            "ansible/roles/services/gitlab/files/docker-compose.yml",
            "services: {}\n",
        ),
        (
            "ansible/roles/services/gitlab/templates/gitlab.env.j2",
            "POSTGRES_HOST={{ a }}\nREDIS_HOST={{ b }}\n",
        ),
        ("ansible/roles/services/postgresql/tasks/main.yml", task),
        ("ansible/roles/services/firefly-iii/tasks/main.yml", task),
        (
            "ansible/roles/services/firefly-importer/tasks/main.yml",
            task,
        ),
        (
            "ansible/roles/services/firefly-importer/templates/env.j2",
            "FIREFLY_III_URL={{ c }}\n",
        ),
    ])
}

#[test]
fn nested_roles_are_each_a_box_in_their_folder() {
    let a = read(&homelab()).unwrap();
    let under_services: Vec<(&str, BoxKind)> = (a.boxes.iter())
        .filter(|b| b.parent.as_deref() == Some("services"))
        .map(|b| (b.guess.title.as_str(), b.guess.kind))
        .collect();
    assert_eq!(
        under_services,
        [
            ("firefly-iii", BoxKind::Service),
            ("firefly-importer", BoxKind::Service),
            ("gitlab", BoxKind::Service),
            ("postgresql", BoxKind::Service),
        ]
    );
    let services = a.boxes.iter().find(|b| b.id == "services").unwrap();
    assert_eq!(services.guess.title, "Services");
    assert!(!a.boxes.iter().any(|b| b.guess.title == "services"));
}

#[test]
fn a_role_with_no_targets_file_deploys_where_its_playbook_says() {
    let a = read(&homelab()).unwrap();
    let common = a.boxes.iter().find(|b| b.guess.title == "common").unwrap();
    assert_eq!(common.guess.kind, BoxKind::Service, "deployed by a play");
    assert_eq!(common.parent.as_deref(), Some("all"));
}

#[test]
fn key_equals_value_settings_draw_arrows_to_the_roles_they_name() {
    let a = read(&homelab()).unwrap();
    let got: Vec<String> = arrows(&a.boxes, &a.graph)
        .iter()
        .filter(|x| x.parent == "services")
        .map(|x| x.id.clone())
        .collect();
    assert_eq!(
        got,
        [
            "services/firefly-importer->services/firefly-iii",
            "services/gitlab->services/postgresql",
        ]
    );
    // a role's entry file says nothing about the link
    let all = arrows(&a.boxes, &a.graph);
    assert_eq!(
        all.iter()
            .find(|x| x.to == "services/postgresql")
            .unwrap()
            .label,
        None
    );
    // no role runs it: an outside system, as before
    assert!(a.boxes.iter().any(|b| b.guess.title == "Redis"));
    assert!(!a.boxes.iter().any(|b| b.guess.title == "PostgreSQL"));
}
