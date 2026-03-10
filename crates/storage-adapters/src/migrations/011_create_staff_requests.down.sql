DROP TRIGGER   IF EXISTS trg_staff_requests_updated_at ON staff_requests;
DROP FUNCTION  IF EXISTS staff_requests_set_updated_at();
DROP INDEX     IF EXISTS idx_staff_req_slug;
DROP INDEX     IF EXISTS idx_staff_req_status;
DROP INDEX     IF EXISTS idx_staff_req_user;
DROP TABLE     IF EXISTS staff_requests;
