/**
 * User Entity Type Definition
 */
export type User = {
  id: string;
  first_name: string;
  last_name: string;
  email: string;
  role: string;
  password?: string;
}

export default User;
