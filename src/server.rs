//! `ChatServer` является агентом. Он ведет список подключенных клиентских сессий.
//! И управляет доступными комнатами. Пиры отправляют сообщения другим пирам в той же комнате через `ChatServer`.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use actix::prelude::*;
use rand::{self, rngs::ThreadRng, Rng};

/// Сервер чата отправляет эти сообщения в сессию
#[derive(Message)]
#[rtype(result = "()")]
pub struct Message(pub String);

/// Сообщение для связи с сервером чата

/// Создается новый сеанс чата
#[derive(Message)]
#[rtype(usize)]
pub struct Connect {
    pub addr: Recipient<Message>,
}

/// Сессия отключена
#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub id: usize,
}

/// Отправить сообщение в определенную комнату
#[derive(Message)]
#[rtype(result = "()")]
pub struct ClientMessage {
    /// Id клиентской сессии
    pub id: usize,
    /// Сообщение сверстника
    pub msg: String,
    /// Название номера
    pub room: String,
}

/// Список доступных номеров
pub struct ListRooms;

impl actix::Message for ListRooms {
    type Result = Vec<String>;
}

/// Присоединитесь к комнате, если комната не существует, создайте новую.
#[derive(Message)]
#[rtype(result = "()")]
pub struct Join {
    /// Client ID
    pub id: usize,

    /// Room name
    pub name: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ChessGame {
    pub id: usize,
    pub step: String,
    pub room: String,
}

impl Handler<ChessGame> for ChatServer {
    type Result = ();

    fn handle(&mut self, msg: ChessGame, _: &mut Context<Self>) {
        self.send_message(&msg.room, msg.step.as_str(), msg.id);
    }
}


/// `ChatServer` управляет чатами и отвечает за координацию чат-сессий.
///
/// Implementation is very naïve.
#[derive(Debug)]
pub struct ChatServer {
    sessions: HashMap<usize, Recipient<Message>>,
    rooms: HashMap<String, HashSet<usize>>,
    rng: ThreadRng,
    visitor_count: Arc<AtomicUsize>,
}

impl ChatServer {
    pub fn new(visitor_count: Arc<AtomicUsize>) -> ChatServer {
        // комната по умолчанию
        let mut rooms = HashMap::new();
        rooms.insert("Main".to_owned(), HashSet::new());

        ChatServer {
            sessions: HashMap::new(),
            rooms,
            rng: rand::thread_rng(),
            visitor_count,
        }
    }
}

impl ChatServer {
    /// Отправить сообщение всем пользователям в комнате
    fn send_message(&self, room: &str, message: &str, skip_id: usize) {
        if let Some(sessions) = self.rooms.get(room) {
            for id in sessions {
                if *id != skip_id {
                    if let Some(addr) = self.sessions.get(id) {
                        let _ = addr.do_send(Message(message.to_owned()));
                    }
                }
            }
        }
    }
}

/// Сделать актера из `ChatServer`
impl Actor for ChatServer {
    /// Мы собираемся использовать простой Context, нам просто нужна способность общаться с другими актерами.
    type Context = Context<Self>;
}

/// Обработчик для сообщения Connect.
///
/// Зарегистрируйте новую сессию и присвойте ей уникальный идентификатор
impl Handler<Connect> for ChatServer {
    type Result = usize;

    fn handle(&mut self, msg: Connect, _: &mut Context<Self>) -> Self::Result {
        println!("Someone joined");

        // уведомлять всех пользователей в одной комнате
        self.send_message("Main", "Someone joined", 0);

        // зарегистрировать сессию со случайным идентификатором
        let id = self.rng.gen::<usize>();
        self.sessions.insert(id, msg.addr);

        // автоматическое присоединение сеанса к главной комнате
        self.rooms
            .entry("Main".to_owned())
            .or_insert_with(HashSet::new)
            .insert(id);

        let count = self.visitor_count.fetch_add(1, Ordering::SeqCst);
        self.send_message("Main", &format!("Total visitors {}", count), 0);

        // отправить идентификатор обратно
        id
    }
}

/// Обработчик сообщения об отключении.
impl Handler<Disconnect> for ChatServer {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Context<Self>) {
        println!("Someone disconnected");

        let mut rooms: Vec<String> = Vec::new();

        // удалить адрес
        if self.sessions.remove(&msg.id).is_some() {
            // удалить сессию из всех помещений
            for (name, sessions) in &mut self.rooms {
                if sessions.remove(&msg.id) {
                    rooms.push(name.to_owned());
                }
            }
        }
        // отправлять сообщения другим пользователям
        for room in rooms {
            self.send_message(&room, "Someone disconnected", 0);
        }
    }
}

/// Обработчик для сообщения Message.
impl Handler<ClientMessage> for ChatServer {
    type Result = ();

    fn handle(&mut self, msg: ClientMessage, _: &mut Context<Self>) {
        self.send_message(&msg.room, msg.msg.as_str(), msg.id);
    }
}

/// Обработчик для сообщения `ListRooms`.
impl Handler<ListRooms> for ChatServer {
    type Result = MessageResult<ListRooms>;

    fn handle(&mut self, _: ListRooms, _: &mut Context<Self>) -> Self::Result {
        let mut rooms = Vec::new();

        for key in self.rooms.keys() {
            rooms.push(key.to_owned())
        }

        MessageResult(rooms)
    }
}

/// Присоединиться к комнате, отправить сообщение о разъединении в старую комнату, отправить сообщение о присоединении в новую комнату
impl Handler<Join> for ChatServer {
    type Result = ();

    fn handle(&mut self, msg: Join, _: &mut Context<Self>) {
        let Join { id, name } = msg;
        let mut rooms = Vec::new();

        // удалить сессию из всех помещений
        for (n, sessions) in &mut self.rooms {
            if sessions.remove(&id) {
                rooms.push(n.to_owned());
            }
        }
        // отправлять сообщения другим пользователям
        for room in rooms {
            self.send_message(&room, "Someone disconnected", 0);
        }

        self.rooms
            .entry(name.clone())
            .or_insert_with(HashSet::new)
            .insert(id);

        self.send_message(&name, "Someone connected", id);
    }
}