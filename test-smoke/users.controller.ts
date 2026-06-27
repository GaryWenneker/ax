import { Controller, Get } from "@nestjs/common";

@Controller("api")
export class UsersController {
  @Get("users")
  listUsers() {
    return [];
  }
}