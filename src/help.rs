//! In-app help: one section per feature, in English and Brazilian Portuguese.
//! Each section can link to the page it describes.

use crate::i18n::Language;
use crate::ui::ActiveTab;

pub struct Section {
    pub title: &'static str,
    pub body: &'static str,
    pub page: Option<ActiveTab>,
}

const fn s(title: &'static str, body: &'static str, page: Option<ActiveTab>) -> Section {
    Section { title, body, page }
}

pub fn sections(lang: Language) -> &'static [Section] {
    match lang {
        Language::EnUs => &EN,
        Language::PtBr => &PT,
    }
}

static EN: [Section; 11] = [
    s(
        "Getting started",
        "Argon Control Center monitors and configures a Raspberry Pi in an Argon40 case. The board, the Argon case and the overclocking limits are detected automatically when the app starts.\n\n\
         Navigate with the sidebar or the top tabs. Hover any value or button to see what it does and which file or command it uses. The footer always shows the action status, argononed, the 5 V supply, IP, uptime and a system summary.",
        None,
    ),
    s(
        "Dashboard",
        "Three styles, chosen in Settings > Dashboard:\n\
         • Compact: btop-style boxes with gradient meters for every core, memory, swap, disks, network, GPU, fan and temperature.\n\
         • Charts: rings and 60-second history charts.\n\
         • Gauges: radial gauges only, including one per core and one per disk.\n\n\
         Dashboards are read-only. Click a ring, gauge or status item to open the page where that value can be changed safely.",
        Some(ActiveTab::Dashboard),
    ),
    s(
        "Fan",
        "Edit the argononed fan curve by dragging its points, or pick a profile (Silent, Standard, Performance, Full speed). Saving writes /etc/argononed.conf and restarts the service.\n\n\
         Manual mode sets a fixed speed directly over I2C. The automatic curve only works while argononed is running.",
        Some(ActiveTab::SmartFan),
    ),
    s(
        "Overclock",
        "Live changes (governor, minimum and maximum clock) apply immediately and are reset at reboot. Firmware settings go to config.txt and apply at the next boot; presets and limits match the detected board.\n\n\
         Safe overclocking:\n\
         • Power-supply guard: settings above factory clocks are blocked while under-voltage has been detected since boot.\n\
         • Trial boot: the new settings boot once from tryboot.txt; if the Pi hangs, switch it off and on to return to the previous settings.\n\
         • Stability test: loads every core while recording temperature, clock and the 5 V supply, then keep or discard the trial.",
        Some(ActiveTab::Overclocking),
    ),
    s(
        "Hardware",
        "Enable or disable I2C, SPI, I2S, UART, audio, 3D acceleration, camera and display detection, the Argon BLSTR DAC and the IR receiver. Changes go to config.txt (the previous file is kept as config.txt.argoncc.bak) and need a reboot.\n\n\
         The case OLED, IR remote and EEPROM power limit are configured here too.",
        Some(ActiveTab::Features),
    ),
    s(
        "GPIO Lab",
        "The 40-pin diagram shows the live level of every GPIO; click a pin to inspect it and drive it high or low. The list view shows the same pins as text.\n\n\
         The console talks to serial ports (UART, USB adapters) and has I2C commands: type help to list them. Use Unlock hardware once to give your user access to GPIO and I2C.",
        Some(ActiveTab::GpioLab),
    ),
    s(
        "Services",
        "argononed runs the automatic fan curve, the case power button and the OLED display. Start, stop, restart it or choose whether it starts at boot. Stopping it leaves the fan at its last speed and disables the power button.",
        Some(ActiveTab::ServicesAndConfig),
    ),
    s(
        "GPU and firmware",
        "GPU load per queue (render, geometry, compute) comes from the kernel's gpu_stats, and CMA memory from /proc/meminfo. With access to vcgencmd the app also shows live GPU, core and video clocks, core voltage, the GPU memory split and the firmware throttling flags (under-voltage, capped clock, throttling, temperature limit, now and since boot).",
        Some(ActiveTab::Dashboard),
    ),
    s(
        "Desktop widgets (KDE Plasma)",
        "Right-click the desktop or a panel, choose Add Widgets and search for Argon. Eight widgets are available: CPU, Thermal, GPU, Memory, Network, Storage, Power and System.\n\n\
         In a widget's Configure dialog, choose the style: bars, gauges (transparent background, sized to the widget) or a chart of the last two minutes. In a panel the widgets show a compact value.",
        None,
    ),
    s(
        "Command line",
        "argon-control-center --status    print telemetry, detected hardware and overclock\n\
         argon-control-center --json      one-line JSON snapshot for scripts and widgets\n\
         argon-control-center --doctor    check which hardware interfaces your user can use\n\
         argon-control-center --stress 180    stability test without the interface\n\
         argon-control-center --pt | --en    start in Portuguese or English",
        None,
    ),
    s(
        "Permissions and troubleshooting",
        "Settings > About > Hardware access (or --doctor) lists what your user can use and how to enable the rest. After the permission setup, log out and back in so new groups apply.\n\n\
         If the Pi does not boot after a firmware change, edit config.txt on the boot media from another computer: restore config.txt.argoncc.bak or remove the ARGON CONTROL CENTER block. Repeated under-voltage means the power supply is too weak: use a 5.1 V / 3 A supply with a short cable.",
        Some(ActiveTab::Settings),
    ),
];

static PT: [Section; 11] = [
    s(
        "Primeiros passos",
        "O Argon Control Center monitora e configura um Raspberry Pi em gabinete Argon40. A placa, o gabinete e os limites de overclock são detectados automaticamente na abertura.\n\n\
         Navegue pela barra lateral ou pelas abas no topo. Passe o mouse sobre qualquer valor ou botão para ver o que ele faz e qual arquivo ou comando usa. O rodapé mostra sempre o status das ações, o argononed, a fonte de 5 V, IP, tempo ligado e um resumo do sistema.",
        None,
    ),
    s(
        "Painel",
        "Três estilos, escolhidos em Configurações > Painel:\n\
         • Compacto: caixas estilo btop com barras em gradiente para cada núcleo, memória, swap, discos, rede, GPU, ventoinha e temperatura.\n\
         • Gráficos: anéis e gráficos de histórico de 60 segundos.\n\
         • Medidores: apenas medidores radiais, incluindo um por núcleo e um por disco.\n\n\
         Os painéis são só de leitura. Clique em um anel, medidor ou item de status para abrir a página onde aquele valor pode ser alterado com segurança.",
        Some(ActiveTab::Dashboard),
    ),
    s(
        "Ventoinha",
        "Edite a curva da ventoinha do argononed arrastando os pontos ou escolha um perfil (Silencioso, Padrão, Desempenho, Máxima). Salvar grava /etc/argononed.conf e reinicia o serviço.\n\n\
         O modo manual define uma rotação fixa direto pelo I2C. A curva automática só funciona com o argononed em execução.",
        Some(ActiveTab::SmartFan),
    ),
    s(
        "Overclock",
        "Mudanças ao vivo (governor, clock mínimo e máximo) valem na hora e voltam ao padrão no reboot. Ajustes de firmware vão para o config.txt e valem no próximo boot; perfis e limites seguem a placa detectada.\n\n\
         Overclock seguro:\n\
         • Trava da fonte: ajustes acima do padrão de fábrica ficam bloqueados se houve subtensão desde o boot.\n\
         • Boot de teste: os novos ajustes iniciam uma única vez pelo tryboot.txt; se o Pi travar, desligue e ligue para voltar aos ajustes anteriores.\n\
         • Teste de estabilidade: carrega todos os núcleos registrando temperatura, clock e a fonte de 5 V; depois mantenha ou descarte o teste.",
        Some(ActiveTab::Overclocking),
    ),
    s(
        "Hardware",
        "Ative ou desative I2C, SPI, I2S, UART, áudio, aceleração 3D, detecção de câmera e display, o DAC BLSTR e o receptor IR do Argon. As mudanças vão para o config.txt (o arquivo anterior fica em config.txt.argoncc.bak) e exigem reboot.\n\n\
         O visor OLED, o controle IR e o limite de corrente da EEPROM também são configurados aqui.",
        Some(ActiveTab::Features),
    ),
    s(
        "GPIO Lab",
        "O diagrama de 40 pinos mostra o nível de cada GPIO ao vivo; clique em um pino para inspecioná-lo e colocá-lo em HIGH ou LOW. A visão em lista mostra os mesmos pinos em texto.\n\n\
         O console fala com portas seriais (UART, adaptadores USB) e tem comandos de I2C: digite help para listá-los. Use Liberar hardware uma vez para dar ao seu usuário acesso a GPIO e I2C.",
        Some(ActiveTab::GpioLab),
    ),
    s(
        "Serviços",
        "O argononed controla a curva automática da ventoinha, o botão power do gabinete e o visor OLED. Inicie, pare, reinicie ou defina se ele sobe no boot. Pará-lo deixa a ventoinha na última rotação e desativa o botão power.",
        Some(ActiveTab::ServicesAndConfig),
    ),
    s(
        "GPU e firmware",
        "A carga da GPU por fila (render, geometria, computação) vem do gpu_stats do kernel, e a memória CMA do /proc/meminfo. Com acesso ao vcgencmd o app também mostra os clocks ao vivo de GPU, core e vídeo, a tensão do core, a divisão de memória da GPU e as flags de limitação do firmware (subtensão, clock limitado, throttling, limite térmico, agora e desde o boot).",
        Some(ActiveTab::Dashboard),
    ),
    s(
        "Widgets da área de trabalho (KDE Plasma)",
        "Clique com o botão direito na área de trabalho ou em um painel, escolha Adicionar widgets e busque por Argon. São oito widgets: CPU, Térmico, GPU, Memória, Rede, Armazenamento, Energia e Sistema.\n\n\
         Em Configurar de cada widget, escolha o estilo: barras, medidores (fundo transparente, ajustados ao tamanho do widget) ou gráfico dos últimos dois minutos. No painel os widgets mostram um valor compacto.",
        None,
    ),
    s(
        "Linha de comando",
        "argon-control-center --status    telemetria, hardware detectado e overclock\n\
         argon-control-center --json      leitura em JSON de uma linha para scripts e widgets\n\
         argon-control-center --doctor    o que o seu usuário consegue acessar no hardware\n\
         argon-control-center --stress 180    teste de estabilidade sem interface\n\
         argon-control-center --pt | --en    abre em português ou inglês",
        None,
    ),
    s(
        "Permissões e solução de problemas",
        "Configurações > Sobre > Acesso ao hardware (ou --doctor) mostra o que o seu usuário pode usar e como habilitar o resto. Depois da configuração de permissões, saia e entre na sessão para os novos grupos valerem.\n\n\
         Se o Pi não iniciar depois de uma mudança no firmware, edite o config.txt da mídia de boot em outro computador: restaure o config.txt.argoncc.bak ou remova o bloco ARGON CONTROL CENTER. Subtensão repetida indica fonte fraca: use uma fonte de 5,1 V / 3 A com cabo curto.",
        Some(ActiveTab::Settings),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_languages_have_the_same_sections_and_links() {
        let (en, pt) = (sections(Language::EnUs), sections(Language::PtBr));
        assert_eq!(en.len(), pt.len());
        for (a, b) in en.iter().zip(pt) {
            assert_eq!(a.page, b.page, "{} / {}", a.title, b.title);
            assert!(!a.body.is_empty() && !b.body.is_empty());
        }
    }
}
