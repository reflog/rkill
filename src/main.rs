use colored::*;
use heim::process::{Pid, Process};
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};
use seahorse::App;
use smol::stream::StreamExt;
use std::convert::TryInto;
use std::env;

#[derive(Debug)]
struct ProcessPort {
    kind: &'static str,
    process: Process,
    port: u16,
}

fn ports_to_processes() -> Vec<ProcessPort> {
    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP | ProtocolFlags::UDP;
    let sockets_info = get_sockets_info(af_flags, proto_flags).expect("Cannot get socket info");
    let mut result: Vec<ProcessPort> = vec![];
    smol::block_on(async {
        for si in sockets_info {
            if si.associated_pids.len() == 0 {
                continue;
            }
            let pid: Pid = si.associated_pids[0].try_into().unwrap();
            if let Ok(p) = heim::process::get(pid).await {
                match si.protocol_socket_info {
                    ProtocolSocketInfo::Tcp(tcp_si) => result.push(ProcessPort {
                        port: tcp_si.local_port,
                        kind: "TCP",
                        process: p,
                    }),
                    ProtocolSocketInfo::Udp(udp_si) => result.push(ProcessPort {
                        port: udp_si.local_port,
                        kind: "UDP",
                        process: p,
                    }),
                }
            }
        }
    });
    result
}

fn kill_process_by_port(arg: String, ports_processes: &Vec<ProcessPort>) -> Result<String, String> {
    let port: u16 = arg
        .parse()
        .map_err(|_| format!("Cannot parse port '{}' as number", arg))?;
    if let Some(result) = ports_processes.iter().find(|p| p.port == port) {
        smol::block_on(async {
            result
                .process
                .kill()
                .await
                .map_err(|_| "Cannot kill process".to_string())?;
            Ok(format!(
                "Process holding port :{} ({}) killed successfully!",
                port, result.kind
            ))
        })
    } else {
        Err(format!(
            "Process that holds port :{} could not be found.",
            port
        ))
    }
}

fn kill_process_by_pid(arg: String) -> Result<String, String> {
    smol::block_on(async {
        if let Ok(pid) = arg.parse() {
            if let Ok(process) = heim::process::get(pid).await {
                process
                    .kill()
                    .await
                    .map_err(|_| "Cannot kill process".to_string())?;
                Ok(format!("Process with pid {} killed successfully!", pid))
            } else {
                Err(format!("Cannot get process with pid {}", pid))
            }
        } else {
            let processes = heim::process::processes()
                .await
                .map_err(|_| "Cannot collect process list".to_string())?;
            futures::pin_mut!(processes);
            while let Some(process) = processes.next().await {
                if let Ok(p) = process {
                    if let Ok(n) = p.name().await {
                        if n == arg {
                            p.kill()
                                .await
                                .map_err(|_| "Cannot kill process".to_string())?;
                            return Ok(format!("Process with name '{}' killed successfully!", arg));
                        }
                    }
                }
            }
            Err(format!("Cannot find process with name '{}'", arg))
        }
    })
}

fn kill_process_by_arg(args: &Vec<String>) -> Vec<Result<String, String>> {
    let ports_processes = ports_to_processes();
    args.iter()
        .map(|arg| {
            if arg.starts_with(":") {
                let real_arg = &arg[1..];
                kill_process_by_port(real_arg.to_string(), &ports_processes)
            } else {
                kill_process_by_pid(arg.to_string())
            }
        })
        .collect()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let app = App::new(env!("CARGO_PKG_NAME"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .version(env!("CARGO_PKG_VERSION"))
        .usage("rkill 1234 7777 nc  # to kill processes by PID or name\n\trkill :1234 :7777   # to kill processes by port number\n\trkill               # run interactively")
        .action(|c| {
            if c.args.len() == 0 {
                c.help();
                return
            };
            kill_process_by_arg(&c.args).iter().for_each(|result| {
                match result {
                    Ok(ok) => println!("{} {}","\u{2705}".green(),ok),
                    Err(err) => println!("{} {}","\u{274C}".red(), err)
                }
            })
        });

    app.run(args);
}
