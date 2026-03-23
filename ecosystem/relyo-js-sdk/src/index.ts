import * as ed from "@noble/ed25519";
import { sha512 } from "@noble/hashes/sha2.js";
ed.etc.sha512Sync = (...m) => sha512(ed.etc.concatBytes(...m));

import { sha3_256 } from "js-sha3";
import bs58 from "bs58";
import axios from "axios";

// IMPORTANT: Updated for pure PoS, no PoW. Matches Rust core exactly.
const CHAIN_ID = 1;

export enum TransactionType {
  Transfer = "Transfer",
  Stake = "Stake",
  Unstake = "Unstake",
  Slash = "Slash",
  Reward = "Reward",
  Genesis = "Genesis",
  ContractCall = "ContractCall",
}

export function getTxTypeNumeric(t: TransactionType | string): number {
  switch (t) {
    case TransactionType.Transfer:
      return 0;
    case TransactionType.Stake:
      return 1;
    case TransactionType.Unstake:
      return 2;
    case TransactionType.Slash:
      return 3;
    case TransactionType.Reward:
      return 4;
    case TransactionType.Genesis:
      return 5;
    case TransactionType.ContractCall:
      return 6;
    default:
      throw new Error(`Unknown TransactionType: ${t}`);
  }
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) throw new Error("Invalid hex string");
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.substring(i, i + 2), 16);
  }
  return bytes;
}

export class KeyPair {
  public privateKey: Uint8Array;
  public publicKey: Uint8Array;

  constructor(privateKey: Uint8Array, publicKey: Uint8Array) {
    this.privateKey = privateKey;
    this.publicKey = publicKey;
  }

  static fromPrivateKey(hexOrBytes: string | Uint8Array): KeyPair {
    const priv =
      typeof hexOrBytes === "string" ? hexToBytes(hexOrBytes) : hexOrBytes;
    const pub = ed.getPublicKey(priv);
    return new KeyPair(priv, pub);
  }

  static generate(): KeyPair {
    // Correct implementation for Noble Ed25519 v2
    const priv = ed.utils.randomPrivateKey();
    const pub = ed.getPublicKey(priv);
    return new KeyPair(priv, pub);
  }
}

export function deriveAddress(
  publicKey: Uint8Array,
  type: number = 0x01,
): string {
  const raw = new Uint8Array(26);
  raw[0] = 0x52; // 'R'
  raw[1] = type;

  const pubHash = new Uint8Array(sha3_256.arrayBuffer(publicKey));
  raw.set(pubHash.slice(0, 20), 2);

  const checksumHash = new Uint8Array(sha3_256.arrayBuffer(raw.slice(0, 22)));
  raw.set(checksumHash.slice(0, 4), 22);

  return bs58.encode(raw);
}

export interface TransactionPayload {
  tx_type: string | TransactionType;
  sender: string;
  receiver: string;
  amount: bigint | number;
  fee: bigint | number;
  timestamp: bigint | number;
  nonce: bigint | number;
  parent_1: string;
  parent_2: string;
  data?: Uint8Array;
}

export class Transaction {
  public tx_type: string;
  public sender: string;
  public receiver: string;
  public amount: bigint;
  public fee: bigint;
  public timestamp: bigint;
  public nonce: bigint;
  public parent_1: string;
  public parent_2: string;
  public sender_pubkey: string = "00".repeat(32);
  public signature: string = "00".repeat(64);
  public data: Uint8Array;

  constructor(p: TransactionPayload) {
    this.tx_type = p.tx_type as string;
    this.sender = p.sender;
    this.receiver = p.receiver;
    this.amount = typeof p.amount === "bigint" ? p.amount : BigInt(p.amount);
    this.fee = typeof p.fee === "bigint" ? p.fee : BigInt(p.fee);
    this.timestamp =
      typeof p.timestamp === "bigint" ? p.timestamp : BigInt(p.timestamp);
    this.nonce = typeof p.nonce === "bigint" ? p.nonce : BigInt(p.nonce);
    this.parent_1 = p.parent_1;
    this.parent_2 = p.parent_2;
    this.data = p.data || new Uint8Array(0);
  }

  private writeUint64LE(val: bigint): Uint8Array {
    const buf = new ArrayBuffer(8);
    const view = new DataView(buf);
    view.setBigUint64(0, val, true);
    return new Uint8Array(buf);
  }

  private writeUint32LE(val: number): Uint8Array {
    const buf = new ArrayBuffer(4);
    const view = new DataView(buf);
    view.setUint32(0, val, true);
    return new Uint8Array(buf);
  }

  public signableBytes(): Uint8Array {
    const parts: Uint8Array[] = [];

    parts.push(new Uint8Array([getTxTypeNumeric(this.tx_type)]));
    parts.push(new TextEncoder().encode(this.sender));
    parts.push(new TextEncoder().encode(this.receiver));
    parts.push(this.writeUint64LE(this.amount));
    parts.push(this.writeUint64LE(this.fee));
    parts.push(this.writeUint64LE(this.timestamp));
    parts.push(this.writeUint64LE(this.nonce));
    parts.push(hexToBytes(this.parent_1));
    parts.push(hexToBytes(this.parent_2));
    parts.push(hexToBytes(this.sender_pubkey));
    parts.push(this.writeUint32LE(CHAIN_ID));

    if (this.data && this.data.length > 0) {
      parts.push(this.writeUint32LE(this.data.length));
      parts.push(this.data);
    }

    const totalLen = parts.reduce((acc, p) => acc + p.length, 0);
    const result = new Uint8Array(totalLen);
    let offset = 0;
    for (const p of parts) {
      result.set(p, offset);
      offset += p.length;
    }
    return result;
  }

  public async sign(keyPair: KeyPair): Promise<void> {
    this.sender_pubkey = bytesToHex(keyPair.publicKey);
    const msg = this.signableBytes();
    const signatureBytes = await ed.signAsync(msg, keyPair.privateKey);
    this.signature = bytesToHex(signatureBytes);
  }

  public toJSON() {
    return {
      tx_type: this.tx_type,
      sender: this.sender,
      receiver: this.receiver,
      amount: Number(this.amount),
      fee: Number(this.fee),
      timestamp: Number(this.timestamp),
      nonce: Number(this.nonce),
      parent_1: this.parent_1,
      parent_2: this.parent_2,
      sender_pubkey: this.sender_pubkey,
      signature: this.signature,
      data: Array.from(this.data),
    };
  }
}

export class RelyoClient {
  private rpcUrl: string;

  constructor(rpcUrl: string) {
    this.rpcUrl = rpcUrl;
  }

  async getBalance(address: string): Promise<number> {
    const res = await axios.post(this.rpcUrl, {
      jsonrpc: "2.0",
      method: "rly_getBalance",
      params: [address],
      id: 1,
    });
    if (res.data.error) throw new Error(res.data.error.message);
    return res.data.result.balance;
  }

  async getNonce(address: string): Promise<number> {
    const res = await axios.post(this.rpcUrl, {
      jsonrpc: "2.0",
      method: "rly_getNonce",
      params: [address],
      id: 1,
    });
    if (res.data.error) throw new Error(res.data.error.message);
    return res.data.result.nonce;
  }

  async getTips(): Promise<{ tip_1: string; tip_2: string }> {
    const res = await axios.post(this.rpcUrl, {
      jsonrpc: "2.0",
      method: "rly_getTips",
      params: [],
      id: 1,
    });
    if (res.data.error) throw new Error(res.data.error.message);

    const tips: string[] = res.data?.result?.tips || [];
    const fallback = "0".repeat(64);

    const tip_1 = tips[0] || fallback;
    const tip_2 = tips[1] || tip_1;

    return { tip_1, tip_2 };
  }

  async submitTransaction(tx: Transaction): Promise<string> {
    const res = await axios.post(this.rpcUrl, {
      jsonrpc: "2.0",
      method: "rly_submitTransaction",
      params: [tx.toJSON()],
      id: 1,
    });
    if (res.data.error) throw new Error(res.data.error.message);
    return res.data.result.tx_hash;
  }
}

export class RelyoWsClient {
  private url: string;
  private ws: any = null;
  private reconnectTimer: any = null;
  private backoff = 2000;
  public onEvent: (event: any) => void = () => {};

  constructor(nodeUrl: string) {
    this.url = nodeUrl.replace(/^http/, "ws") + "/ws";
  }

  connect() {
    if (typeof WebSocket !== "undefined") {
      this.ws = new WebSocket(this.url);
    } else {
      const WS = require("ws");
      this.ws = new WS(this.url);
    }

    this.ws.onopen = () => {
      console.log("Connected to Relyo Node WS");
      this.backoff = 2000;
    };

    this.ws.onmessage = (msg: any) => {
      try {
        const data = JSON.parse(msg.data);
        this.onEvent(data);
      } catch (e) {}
    };

    this.ws.onclose = () => {
      console.log("WS connection dropped. Reconnecting...");
      this.scheduleReconnect();
    };

    this.ws.onerror = (e: any) => {
      if (this.ws) this.ws.close();
    };
  }

  private scheduleReconnect() {
    if (this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.backoff = Math.min(this.backoff * 2, 30000);
      this.connect();
    }, this.backoff);
  }
}
