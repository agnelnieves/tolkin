// Self-authored fixture for the lossy/repomix comparison row.
// Realistic shape (interfaces, JSDoc, function bodies, imports), small
// enough to keep the fixture readable, large enough that repomix --compress
// can drop function bodies and produce a measurable delta.
//
// Provenance: written for the tolkin benchmark by the repository owner;
// licensed MIT alongside the rest of the benchmark fixtures.

import { db } from "./db";
import { cache } from "./cache";
import * as argon2 from "argon2";
import { metrics } from "./metrics";

export interface User {
  id: string;
  email: string;
  passwordHash: string;
  createdAt: Date;
  lastLoginAt: Date | null;
}

export interface Session {
  token: string;
  userId: string;
  expiresAt: Date;
}

/**
 * Authenticate a user with email and password.
 * Returns a session on success, null on auth failure.
 * Records both successful logins (for audit) and failed attempts (for rate limiting).
 */
export async function authenticate(email: string, password: string): Promise<Session | null> {
  metrics.inc("auth.login.attempt");
  const user = await findUserByEmail(email);
  if (!user) {
    metrics.inc("auth.login.failed", { reason: "unknown_email" });
    await recordFailedAttempt(email);
    return null;
  }
  const match = await comparePasswords(password, user.passwordHash);
  if (!match) {
    metrics.inc("auth.login.failed", { reason: "bad_password" });
    await recordFailedAttempt(email);
    return null;
  }
  await recordSuccessfulLogin(user);
  const session = await createSession(user);
  metrics.inc("auth.login.success");
  return session;
}

async function findUserByEmail(email: string): Promise<User | null> {
  const normalized = email.trim().toLowerCase();
  const cached = await cache.get(`user:email:${normalized}`);
  if (cached) {
    return JSON.parse(cached) as User;
  }
  const row = await db.users.findFirst({ where: { email: normalized } });
  if (!row) return null;
  await cache.set(`user:email:${normalized}`, JSON.stringify(row), { ttl: 60 });
  return row;
}

async function comparePasswords(plain: string, stored: string): Promise<boolean> {
  const [version, hash] = stored.split(":", 2);
  if (version === "argon2") {
    try {
      return await argon2.verify(hash, plain);
    } catch (err) {
      metrics.inc("auth.password.verify_error", { algo: "argon2" });
      return false;
    }
  }
  if (version === "bcrypt") {
    const bcrypt = await import("bcrypt");
    return await bcrypt.compare(plain, hash);
  }
  throw new Error(`unknown password hash version: ${version}`);
}

async function recordSuccessfulLogin(user: User): Promise<void> {
  await db.loginEvents.create({
    data: {
      userId: user.id,
      occurredAt: new Date(),
      source: "credential",
      outcome: "success",
    },
  });
  await db.users.update({
    where: { id: user.id },
    data: { lastLoginAt: new Date() },
  });
}

async function recordFailedAttempt(email: string): Promise<void> {
  const key = `auth:failed:${email.toLowerCase()}`;
  const current = await cache.get(key);
  const count = current ? parseInt(current, 10) + 1 : 1;
  await cache.set(key, String(count), { ttl: 900 });
  if (count >= 5) {
    metrics.inc("auth.login.locked", { email });
  }
}

async function createSession(user: User): Promise<Session> {
  const token = crypto.randomUUID();
  const expiresAt = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000);
  await db.sessions.create({
    data: { token, userId: user.id, expiresAt },
  });
  return { token, userId: user.id, expiresAt };
}
